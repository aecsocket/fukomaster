#![doc = include_str!("../README.md")]

// mod util;

use std::{
    collections::hash_map::Entry,
    fs,
    os::unix::fs::{FileTypeExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use ahash::AHashMap;
use anyhow::{anyhow, Context, Result};
use clap::Parser;

use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AbsInfo, AbsoluteAxisType, AttributeSet, Device, EventStream, EventType, InputEvent,
    InputEventKind, Key, PropType, UinputAbsSetup,
};
use futures::{stream::FuturesUnordered, StreamExt};
use log::{debug, error, info, warn};
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

type Never = std::convert::Infallible;

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

    simulate_trackpad(
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
    builder.filter_level(log::LevelFilter::Info);
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
            abs_with_max(4),
        ))? // max 5 touches
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

async fn simulate_trackpad(
    device_events: &mut mpsc::UnboundedReceiver<DeviceEvent>,
    sink: &mut VirtualDevice,
    input_allow: &[PathBuf],
    input_deny: &[PathBuf],
    swipe_2: Option<Key>,
    swipe_3: Option<Key>,
    swipe_4: Option<Key>,
    swipe_5: Option<Key>,
    x_mult: f32,
    y_mult: f32,
    grab: bool,
) -> Result<Never> {
    enum State {
        Normal,
        Swiping { fingers: u8, x: i32, y: i32 },
    }

    impl State {
        fn swiping(fingers: u8) -> Self {
            Self::Swiping {
                fingers,
                x: 0,
                y: 0,
            }
        }
    }

    let mut state = State::Normal;
    let mut devices = AHashMap::<PathBuf, EventStream>::new();

    loop {
        let mut input_events = devices
            .iter_mut()
            .map(|(path, events)| async move {
                let res = events.next_event().await;
                (path, events.device_mut(), res)
            })
            .collect::<FuturesUnordered<_>>();

        tokio::select! {
            Some(event) = device_events.recv() => {
                drop(input_events);
                match event {
                    DeviceEvent::Added(path) => {
                        info!("Added {path:?}");
                    }
                    DeviceEvent::Removed(path) => {
                        if let Some(events) = devices.remove(&path) {
                            if let Some(name) = events.device().name() {
                                info!("Removed {name:?} ({path:?})");
                            } else {
                                info!("Removed {path:?}");
                            }
                        }
                    }
                }
            }
            Some((path, device, input)) = input_events.next() => {
                let input = match input {
                    Ok(input) => input,
                    Err(err) => {
                        warn!(
                            "Failed to read events from {path:?}: {:#}",
                            anyhow::Error::new(err)
                        );
                        continue;
                    }
                };

                match state {
                    State::Normal => {
                        let mut try_start_swipe = |trigger: Option<Key>, fingers, btn_tool| -> Result<()> {
                            if trigger
                                .map(|key| input.kind() == InputEventKind::Key(key))
                                .unwrap_or(false)
                                && input.value() == 1
                            {
                                if grab {
                                    if let Err(err) = device.grab() {
                                        warn!(
                                            "Failed to grab {path:?}, will not start swiping: {:#}",
                                            anyhow::Error::new(err)
                                        );
                                        return Ok(());
                                    }
                                }

                                start_swipe(sink, fingers, btn_tool)?;
                                info!("Start swipe with {fingers} fingers on {path:?}");
                                state = State::Swiping {
                                    fingers,
                                    x: 0,
                                    y: 0,
                                };
                            }
                            Ok(())
                        };

                        try_start_swipe(swipe_2, 2, Key::BTN_TOOL_DOUBLETAP)?;
                        try_start_swipe(swipe_3, 3, Key::BTN_TOOL_TRIPLETAP)?;
                        try_start_swipe(swipe_4, 4, Key::BTN_TOOL_QUADTAP)?;
                        try_start_swipe(swipe_5, 5, Key::BTN_TOOL_QUINTTAP)?;
                    }
                    State::Swiping { fingers, x, y } => {}
                }
            }
        }

        // match (state, input) {
        //     (State::Normal, event) if matches!(event.kind(), InputEventKind::Key(key)) => {}
        //     (State::Normal, event)
        //         if swipe_3.map(|key| event.kind() == InputEventKind::Key(key))(
        //             State::Normal | State::Swiping { .. },
        //             _,
        //         ) => {}
        // }
    }

    /*

    loop {
        tokio::select! {
            event = recv_device_event.recv() => {
                let Some(event) = event else {
                    error!("{DEV_INPUT:?} watcher is closed - unable to read any more device changes");
                    continue;
                };

                match event {
                    DeviceEvent::Added(path) => add_device(path.clone(), &mut devices, sink_devnode, &input_allow, &input_deny)?,
                    DeviceEvent::Removed(path) => {
                        debug!("Received signal to remove {path:?}");
                        if let Some(device) = devices.remove(&path) {
                            if let Some(name) = device.name() {
                                info!("Removed {name:?} ({path:?})");
                            } else {
                                info!("Removed {path:?}");
                            }
                        }
                    }
                }
            }
        }
    }*/
}

/*
fn add_device(
    path: PathBuf,
    devices: &mut AHashMap<PathBuf, Device>,
    sink_devnode: &str,
    input_allow: &[PathBuf],
    input_deny: &[PathBuf],
) -> Result<()> {
    const DEVICE_PREFIX: &str = "event";

    debug!("Received request to add {path:?}");

    let path_str = path.to_str().with_context(|| "path is not UTF-8")?;
    if sink_devnode == path_str {
        debug!("  Rejected: this is our own sink device");
        return Ok(());
    }

    let file_name = path
        .file_name()
        .with_context(|| "path does not have a file name")?
        .to_str()
        .with_context(|| "file name is not UTF-8")?;
    if !file_name.starts_with(DEVICE_PREFIX) {
        debug!("  Rejected: path does not start with {DEVICE_PREFIX:?}");
        return Ok(());
    }

    if input_deny.contains(&path) {
        debug!("  Rejected: path is in the deny list");
        return Ok(());
    }

    if !input_allow.is_empty() && !input_allow.contains(&path) {
        debug!("  Rejected: path is not in the allow list");
        return Ok(());
    }

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&path)
        .with_context(|| "failed to open device file")?;
    let metadata = file
        .metadata()
        .with_context(|| "failed to read file metadata")?;
    if !metadata.file_type().is_char_device() {
        debug!("  Rejected: file is not a char device");
        return Ok(());
    }

    let Entry::Vacant(entry) = devices.entry(path.clone()) else {
        return Err(anyhow!("device {path:?} is already being tracked"));
    };

    let device =
        Device::new_from_path(path.clone()).with_context(|| "failed to open device file")?;
    let device = entry.insert(device);

    if let Some(name) = device.name() {
        info!("Added {name:?} ({path:?})");
    } else {
        info!("Added {path:?}");
    }
    Ok(())
}*/

/*
#[derive(Debug, Clone, Copy)]
struct Config {
    fingers: i32,
    trigger_key: EV_KEY,
    btn_tool: EV_KEY,
    x_mult: f32,
    y_mult: f32,
    grab: bool,
    start_swiping: bool,
}

fn simulate_trackpad(source: &mut Device, sink: &UInputDevice, config: Config) -> Result<()> {
    enum State {
        Normal,
        Swiping { x: i32, y: i32 },
    }

    impl State {
        fn swiping() -> Self {
            Self::Swiping { x: 0, y: 0 }
        }
    }

    let Config {
        fingers,
        trigger_key,
        btn_tool,
        x_mult,
        y_mult,
        grab,
        start_swiping,
    } = config;

    let mut state = if start_swiping {
        start_swipe(source, sink, fingers, btn_tool, grab)
            .with_context(|| "failed to start swipe")?;
        State::swiping()
    } else {
        State::Normal
    };

    loop {
        let (_, input) = source
            .next_event(ReadFlag::NORMAL | ReadFlag::BLOCKING)
            .with_context(|| "failed to read next event from source device")?;

        match (&mut state, input.event_code, input.value) {
            (State::Normal, EventCode::EV_KEY(key), 1) if key == trigger_key => {
                start_swipe(source, sink, fingers, btn_tool, grab)
                    .with_context(|| "failed to start swipe")?;
                state = State::swiping();
            }

            (State::Swiping { .. }, EventCode::EV_KEY(key), 0) if key == trigger_key => {
                stop_swipe(source, sink, fingers, btn_tool, grab)
                    .with_context(|| "failed to stop swipe")?;
                state = State::Normal;
            }
            (State::Swiping { ref mut x, y }, EventCode::EV_REL(REL_X), dx) => {
                *x += dx;
                update_position(sink, fingers, *x, x_mult, *y, y_mult)?;
            }
            (State::Swiping { x, ref mut y }, EventCode::EV_REL(REL_Y), dy) => {
                *y += dy;
                update_position(sink, fingers, *x, x_mult, *y, y_mult)?;
            }

            (State::Normal | State::Swiping { .. }, _, _) => {}
        }
    }
}*/

fn abs_event(axis_type: AbsoluteAxisType, value: i32) -> InputEvent {
    InputEvent::new_now(EventType::ABSOLUTE, axis_type.0, value)
}

fn start_swipe(sink: &mut VirtualDevice, fingers: u8, btn_tool: Key) -> Result<()> {
    /*
    E: 0.000001 0003 0039 8661	# EV_ABS / ABS_MT_TRACKING_ID   8661
    E: 0.000001 0003 0035 0690	# EV_ABS / ABS_MT_POSITION_X    690
    E: 0.000001 0003 0036 0665	# EV_ABS / ABS_MT_POSITION_Y    665
    E: 0.000001 0003 002f 0001	# EV_ABS / ABS_MT_SLOT          1
    E: 0.000001 0003 0039 8662	# EV_ABS / ABS_MT_TRACKING_ID   8662
    E: 0.000001 0003 0035 0881	# EV_ABS / ABS_MT_POSITION_X    881
    E: 0.000001 0003 0036 0306	# EV_ABS / ABS_MT_POSITION_Y    306
    E: 0.000001 0003 002f 0002	# EV_ABS / ABS_MT_SLOT          2
    E: 0.000001 0003 0039 8663	# EV_ABS / ABS_MT_TRACKING_ID   8663
    E: 0.000001 0003 0035 0679	# EV_ABS / ABS_MT_POSITION_X    679
    E: 0.000001 0003 0036 0443	# EV_ABS / ABS_MT_POSITION_Y    443
    E: 0.000001 0001 014a 0001	# EV_KEY / BTN_TOUCH            1
    E: 0.000001 0001 014e 0001	# EV_KEY / BTN_TOOL_TRIPLETAP   1
    E: 0.000001 0003 0000 0690	# EV_ABS / ABS_X                690
    E: 0.000001 0003 0001 0665	# EV_ABS / ABS_Y                665
    E: 0.000001 0004 0005 0000	# EV_MSC / MSC_TIMESTAMP        0
    E: 0.000001 0000 0000 0000	# ------------ SYN_REPORT (0) ---------- +0ms
    */

    let events = (0..i32::from(fingers))
        .flat_map(|finger| {
            [
                abs_event(AbsoluteAxisType::ABS_MT_SLOT, finger),
                abs_event(AbsoluteAxisType::ABS_MT_TRACKING_ID, finger),
                abs_event(AbsoluteAxisType::ABS_MT_POSITION_X, 0),
                abs_event(AbsoluteAxisType::ABS_MT_POSITION_Y, 0),
            ]
        })
        .chain([
            InputEvent::new(EventType::KEY, Key::BTN_TOUCH.0, 1),
            InputEvent::new(EventType::KEY, btn_tool.0, 1),
        ]);

    sink.emit(&events.collect::<Vec<_>>())?;
    Ok(())
}

fn update_position(
    sink: &mut VirtualDevice,
    fingers: u8,
    x: i32,
    x_mult: f32,
    y: i32,
    y_mult: f32,
) -> Result<()> {
    /*
    E: 0.020080 0003 002f 0000	# EV_ABS / ABS_MT_SLOT          0
    E: 0.020080 0003 0035 0686	# EV_ABS / ABS_MT_POSITION_X    686
    E: 0.020080 0003 002f 0001	# EV_ABS / ABS_MT_SLOT          1
    E: 0.020080 0003 0035 0878	# EV_ABS / ABS_MT_POSITION_X    878
    E: 0.020080 0003 002f 0002	# EV_ABS / ABS_MT_SLOT          2
    E: 0.020080 0003 0035 0675	# EV_ABS / ABS_MT_POSITION_X    675
    E: 0.020080 0003 0036 0442	# EV_ABS / ABS_MT_POSITION_Y    442
    E: 0.020080 0003 0000 0686	# EV_ABS / ABS_X                686
    E: 0.020080 0004 0005 21000	# EV_MSC / MSC_TIMESTAMP        21000
    E: 0.020080 0000 0000 0000	# ------------ SYN_REPORT (0) ---------- +7ms
    */

    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let x = ((x as f32) * x_mult) as i32;
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let y = ((y as f32) * y_mult) as i32;

    let events = (0..i32::from(fingers)).flat_map(|finger| {
        [
            abs_event(AbsoluteAxisType::ABS_MT_SLOT, finger),
            abs_event(AbsoluteAxisType::ABS_MT_POSITION_X, x),
            abs_event(AbsoluteAxisType::ABS_MT_POSITION_Y, y),
        ]
    });

    sink.emit(&events.collect::<Vec<_>>())?;
    Ok(())
}

fn stop_swipe(sink: &mut VirtualDevice, fingers: u8, btn_tool: Key) -> Result<()> {
    /*
    E: 2.992985 0000 0000 0000	# ------------ SYN_REPORT (0) ---------- +7ms
    E: 3.000143 0003 002f 0001	# EV_ABS / ABS_MT_SLOT          1
    E: 3.000143 0003 0039 -001	# EV_ABS / ABS_MT_TRACKING_ID   -1
    E: 3.000143 0003 002f 0002	# EV_ABS / ABS_MT_SLOT          2
    E: 3.000143 0003 0039 -001	# EV_ABS / ABS_MT_TRACKING_ID   -1
    E: 3.000143 0001 0145 0001	# EV_KEY / BTN_TOOL_FINGER      1
    E: 3.000143 0001 014e 0000	# EV_KEY / BTN_TOOL_TRIPLETAP   0
    E: 3.000143 0004 0005 2942200	# EV_MSC / MSC_TIMESTAMP        2942200
    E: 3.000143 0000 0000 0000	# ------------ SYN_REPORT (0) ---------- +8ms
    E: 3.007174 0003 002f 0000	# EV_ABS / ABS_MT_SLOT          0
    E: 3.007174 0003 0039 -001	# EV_ABS / ABS_MT_TRACKING_ID   -1
    E: 3.007174 0001 014a 0000	# EV_KEY / BTN_TOUCH            0
    E: 3.007174 0001 0145 0000	# EV_KEY / BTN_TOOL_FINGER      0
    E: 3.007174 0004 0005 2948400	# EV_MSC / MSC_TIMESTAMP        2948400
    E: 3.007174 0000 0000 0000	# ------------ SYN_REPORT (0) ---------- +7ms
    */

    let events = (0..i32::from(fingers))
        .flat_map(|finger| {
            [
                abs_event(AbsoluteAxisType::ABS_MT_SLOT, finger),
                abs_event(AbsoluteAxisType::ABS_MT_TRACKING_ID, -1),
            ]
        })
        .chain([
            InputEvent::new_now(EventType::KEY, Key::BTN_TOOL_FINGER.0, 0),
            InputEvent::new_now(EventType::KEY, btn_tool.0, 0),
        ]);

    sink.emit(&events.collect::<Vec<_>>())?;
    Ok(())
}
