#![doc = include_str!("../README.md")]

mod states;
mod swipe;

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Parser;

use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AbsInfo, AbsoluteAxisType, AttributeSet, Key, PropType, UinputAbsSetup,
};
use futures::never::Never;
use log::{debug, info, warn};
use notify::Watcher;
use tokio::sync::mpsc;

/// Simulate a trackpad with your physical mouse
///
/// When a certain key is pressed (i.e. the mouse gesture button on a mouse),
/// this tool starts a virtual trackpad swipe, and converts mouse movements
/// into finger movements on this virtual trackpad. When the same key is
/// released, the trackpad swipe stops.
///
/// This tool works on `uinput` key codes. Use `wev` to test which button on
/// your mouse you want to use for activation. For the MX Master 3S, the mouse
/// gesture button has key code `277`.
#[derive(Debug, Clone, clap::Parser)]
pub struct Args {
    /// Input device files to read inputs from (e.g. `/dev/input/event1`)
    ///
    /// Without this option, all devices under `/dev/input` will be read for
    /// inputs. If this option is specified, only the given devices will be
    /// read.
    #[arg(short = 'i')]
    pub input_allow: Vec<PathBuf>,
    /// Input device files to *never* read inputs from (e.g. `/dev/input/event1`)
    ///
    /// Any devices given under this option will never be read for inputs, even
    /// if they appear in the `-i` list.
    #[arg(short = 'I')]
    pub input_deny: Vec<PathBuf>,
    /// Key code which activates 2-finger swiping mode
    #[arg(short = '2')]
    pub swipe_2: Option<u16>,
    /// Key code which activates 3-finger swiping mode
    #[arg(short = '3', default_value = "277")]
    pub swipe_3: Option<u16>,
    /// Key code which activates 4-finger swiping mode
    #[arg(short = '4')]
    pub swipe_4: Option<u16>,
    /// Key code which activates 5-finger swiping mode
    #[arg(short = '5')]
    pub swipe_5: Option<u16>,
    /// Resolution of the virtual trackpad
    ///
    /// A larger resolution means you have to move your mouse further to have
    /// the trackpad move the same distance.
    ///
    /// The value is used directly as the resolution of the virtual `uinput`
    /// device.
    #[arg(short, long, default_value_t = 12)]
    pub resolution: u16,
    /// Swipe speed multiplier on the X axis
    #[arg(short, long, default_value_t = 1.0)]
    pub x_mult: f32,
    /// Swipe speed multiplier on the Y axis
    #[arg(short, long, default_value_t = 1.0)]
    pub y_mult: f32,
    /// Disables grabbing the mouse cursor in `evdev` when swiping
    ///
    /// If grabbing is disabled, the mouse cursor will move with the virtual
    /// trackpad when swiping, but may resolve issues with other processes
    /// which also attempt to grab the mouse.
    #[arg(long)]
    pub no_grab: bool,
}

const DEV_INPUT: &str = "/dev/input";
const VIRTUAL_DEVICE_NAME: &str = "fukomaster virtual trackpad";

#[derive(Debug, Clone)]
enum DeviceEvent {
    Added(PathBuf),
    Removed(PathBuf),
}

#[tokio::main]
async fn main() -> Result<Never> {
    init_logging();

    // arg parsing

    let Args {
        input_allow,
        input_deny,
        swipe_2,
        swipe_3,
        swipe_4,
        swipe_5,
        resolution,
        x_mult,
        y_mult,
        no_grab,
    } = Args::parse();

    let swipe_2 = swipe_2.map(Key::new);
    let swipe_3 = swipe_3.map(Key::new);
    let swipe_4 = swipe_4.map(Key::new);
    let swipe_5 = swipe_5.map(Key::new);

    let grab = !no_grab;

    // setup

    let (send_device_events, mut recv_device_events) = mpsc::unbounded_channel::<DeviceEvent>();

    // first enumerate what devices we already have
    // note that paths in DeviceEvents may not actually point to a device;
    // it's the consumer's job to figure out if a path is actually for a device
    // that we can use
    for result in fs::read_dir(DEV_INPUT)
        .with_context(|| format!("failed to list files under {DEV_INPUT:?}"))?
    {
        let entry = result.with_context(|| format!("failed to read file under {DEV_INPUT:?}"))?;
        send_device_events
            .send(DeviceEvent::Added(entry.path()))
            .expect("channel should be open");
    }

    // then set up a watcher to watch for device changes
    let mut dev_watcher = notify::recommended_watcher(move |res| match res {
        Ok(notify::Event {
            kind: notify::EventKind::Create(_),
            paths,
            ..
        }) => {
            for path in paths.into_iter() {
                debug!("{path:?} created");
                let _ = send_device_events.send(DeviceEvent::Added(path));
            }
        }
        Ok(notify::Event {
            kind: notify::EventKind::Remove(_),
            paths,
            ..
        }) => {
            for path in paths.into_iter() {
                debug!("{path:?} removed");
                let _ = send_device_events.send(DeviceEvent::Removed(path));
            }
        }
        Ok(_) => {}
        Err(err) => {
            warn!(
                "Error while watching {DEV_INPUT:?}: {:#}",
                anyhow::Error::new(err)
            );
        }
    })
    .with_context(|| format!("failed to create {DEV_INPUT:?} watcher"))?;

    dev_watcher
        .watch(Path::new(DEV_INPUT), notify::RecursiveMode::NonRecursive)
        .with_context(|| format!("failed to start watching {DEV_INPUT:?}"))?;
    info!("Watching {DEV_INPUT:?} for device changes");

    let mut sink = create_trackpad(resolution).with_context(|| "failed to create sink")?;
    // we need a slight delay after creating the input device
    // so that other processes (i.e. compositor) can recognize it
    tokio::time::sleep(Duration::from_millis(200)).await;
    info!("Created virtual trackpad");
    let sys_path = sink
        .get_syspath()
        .with_context(|| "failed to get sys path of sink")?;
    info!("  sys path = {sys_path:?}");
    let mut dev_nodes = sink
        .enumerate_dev_nodes()
        .await
        .with_context(|| "failed to enumerate dev nodes of sink")?;
    while let Ok(Some(dev_node)) = dev_nodes.next_entry().await {
        info!("  dev node = {dev_node:?}");
    }

    swipe::simulate(
        &mut recv_device_events,
        &mut sink,
        &input_allow,
        &input_deny,
        swipe_2,
        swipe_3,
        swipe_4,
        swipe_5,
        x_mult,
        y_mult,
        grab,
    )
    .await
}

fn init_logging() {
    let mut builder = pretty_env_logger::formatted_timed_builder();
    builder.filter_level(log::LevelFilter::Trace);
    builder.parse_default_env();
    builder.init();
}

fn create_trackpad(resolution: u16) -> Result<VirtualDevice> {
    /*
    # Supported events:
    #   Event type 0 (EV_SYN)
    #     Event code 0 (SYN_REPORT)
    #     Event code 1 (SYN_CONFIG)
    #     Event code 2 (SYN_MT_REPORT)
    #     Event code 3 (SYN_DROPPED)
    #     Event code 4 ((null))
    #     Event code 5 ((null))
    #     Event code 6 ((null))
    #     Event code 7 ((null))
    #     Event code 8 ((null))
    #     Event code 9 ((null))
    #     Event code 10 ((null))
    #     Event code 11 ((null))
    #     Event code 12 ((null))
    #     Event code 13 ((null))
    #     Event code 14 ((null))
    #     Event code 15 (SYN_MAX)
    #   Event type 1 (EV_KEY)
    #     Event code 272 (BTN_LEFT)
    #     Event code 273 (BTN_RIGHT)
    #     Event code 325 (BTN_TOOL_FINGER)
    #     Event code 328 (BTN_TOOL_QUINTTAP)
    #     Event code 330 (BTN_TOUCH)
    #     Event code 333 (BTN_TOOL_DOUBLETAP)
    #     Event code 334 (BTN_TOOL_TRIPLETAP)
    #     Event code 335 (BTN_TOOL_QUADTAP)
    #   Event type 3 (EV_ABS)
    #     Event code 0 (ABS_X)
    #       Value      848
    #       Min          0
    #       Max       1337
    #       Fuzz         0
    #       Flat         0
    #       Resolution  12
    #     Event code 1 (ABS_Y)
    #       Value      467
    #       Min          0
    #       Max        876
    #       Fuzz         0
    #       Flat         0
    #       Resolution  12
    #     Event code 47 (ABS_MT_SLOT)
    #       Value        0
    #       Min          0
    #       Max          4
    #       Fuzz         0
    #       Flat         0
    #       Resolution   0
    #     Event code 53 (ABS_MT_POSITION_X)
    #       Value        0
    #       Min          0
    #       Max       1337
    #       Fuzz         0
    #       Flat         0
    #       Resolution  12
    #     Event code 54 (ABS_MT_POSITION_Y)
    #       Value        0
    #       Min          0
    #       Max        876
    #       Fuzz         0
    #       Flat         0
    #       Resolution  12
    #     Event code 55 (ABS_MT_TOOL_TYPE)
    #       Value        0
    #       Min          0
    #       Max          2
    #       Fuzz         0
    #       Flat         0
    #       Resolution   0
    #     Event code 57 (ABS_MT_TRACKING_ID)
    #       Value        0
    #       Min          0
    #       Max      65535
    #       Fuzz         0
    #       Flat         0
    #       Resolution   0
    #   Event type 4 (EV_MSC)
    #     Event code 5 (MSC_TIMESTAMP)
    # Properties:
    #   Property  type 0 (INPUT_PROP_POINTER)
    #   Property  type 2 (INPUT_PROP_BUTTONPAD)
    */

    // https://www.kernel.org/doc/html/v4.12/input/event-codes.html
    // https://www.kernel.org/doc/html/v4.12/input/multi-touch-protocol.html

    fn abs(min: i32, max: i32, resolution: i32) -> AbsInfo {
        AbsInfo::new(0, min, max, 0, 0, resolution)
    }

    fn abs_with_max(max: i32) -> AbsInfo {
        abs(0, max, 0)
    }

    let resolution = i32::from(resolution);
    let dev = VirtualDeviceBuilder::new()?
        .name(VIRTUAL_DEVICE_NAME)
        .with_properties(&AttributeSet::from_iter([PropType::POINTER]))?
        .with_keys(&AttributeSet::from_iter([
            Key::BTN_TOOL_FINGER,
            Key::BTN_TOUCH,
            Key::BTN_TOOL_DOUBLETAP,
            Key::BTN_TOOL_TRIPLETAP,
            Key::BTN_TOOL_QUADTAP,
            Key::BTN_TOOL_QUINTTAP,
        ]))?
        .with_absolute_axis(&UinputAbsSetup::new(
            AbsoluteAxisType::ABS_MT_SLOT,
            abs_with_max(4), // max 5 touches
        ))?
        .with_absolute_axis(&UinputAbsSetup::new(
            AbsoluteAxisType::ABS_MT_TRACKING_ID,
            abs_with_max(i32::MAX),
        ))?
        .with_absolute_axis(&UinputAbsSetup::new(
            AbsoluteAxisType::ABS_MT_POSITION_X,
            abs(i32::MIN, i32::MAX, resolution),
        ))?
        .with_absolute_axis(&UinputAbsSetup::new(
            AbsoluteAxisType::ABS_MT_POSITION_Y,
            abs(i32::MIN, i32::MAX, resolution),
        ))?
        .build()?;
    Ok(dev)
}
