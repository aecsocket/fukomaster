#![doc = include_str!("../README.md")]

mod states;
mod swipe;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Parser;

use evdev::Key;
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

#[derive(Debug, Clone)]
enum NotifyEvent {
    Created(PathBuf),
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

    let (send_notifs, mut recv_notifs) = mpsc::unbounded_channel::<NotifyEvent>();

    // first enumerate what devices we already have
    // note that paths in NotifyEvent may not actually point to a device;
    // it's the consumer's job to figure out if a path is actually for a device
    // that we can use
    for result in fs::read_dir(DEV_INPUT)
        .with_context(|| format!("failed to list files under {DEV_INPUT:?}"))?
    {
        let entry = result.with_context(|| format!("failed to read file under {DEV_INPUT:?}"))?;
        send_notifs
            .send(NotifyEvent::Created(entry.path()))
            .expect("channel should be open");
    }

    // then set up a watcher to watch for device changes
    let mut dev_watcher = notify::recommended_watcher(move |res| match res {
        Ok(notify::Event {
            kind: notify::EventKind::Create(_),
            paths,
            ..
        }) => {
            for path in paths {
                debug!("{path:?} created");
                let _ = send_notifs.send(NotifyEvent::Created(path));
            }
        }
        Ok(notify::Event {
            kind: notify::EventKind::Remove(_),
            paths,
            ..
        }) => {
            for path in paths {
                debug!("{path:?} removed");
                let _ = send_notifs.send(NotifyEvent::Removed(path));
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

    swipe::simulate(
        &mut recv_notifs,
        &input_allow,
        &input_deny,
        swipe_2,
        swipe_3,
        swipe_4,
        swipe_5,
        resolution,
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
