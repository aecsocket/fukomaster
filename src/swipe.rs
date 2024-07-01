use std::{collections::hash_map::Entry, os::linux::raw::stat, path::PathBuf};

use ahash::AHashMap;
use anyhow::{anyhow, Context, Result};
use evdev::{
    uinput::VirtualDevice, Device, EventStream, InputEvent, InputEventKind, Key, RelativeAxisType,
};
use futures::{never::Never, stream::FuturesUnordered, StreamExt};
use log::{debug, info, trace, warn};
use tokio::sync::mpsc;

use crate::{
    states::{Fingers, State, Swiping},
    DeviceEvent,
};

pub async fn simulate(
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
    let mut sink_dev_nodes = Vec::new();
    let mut dev_nodes_iter = sink
        .enumerate_dev_nodes()
        .await
        .with_context(|| "failed to enumerate sink dev nodes")?;
    while let Ok(Some(dev_node)) = dev_nodes_iter.next_entry().await {
        sink_dev_nodes.push(dev_node);
    }

    let mut state = State::default();
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
                state = on_device_event(event, sink, &sink_dev_nodes, input_allow, input_deny, &mut devices, state)?;
            }
            Some((source_path, source, input)) = input_events.next() => {
                let input = match input {
                    Ok(input) => input,
                    Err(err) => {
                        warn!(
                            "Failed to read events from {source_path:?}: {:#}",
                            anyhow::Error::new(err)
                        );
                        continue;
                    }
                };
                state = on_input_event(swipe_2, swipe_3, swipe_4, swipe_5, x_mult, y_mult, grab, source, source_path, sink, input, state)?;
            }
        }
    }
}

fn on_device_event(
    event: DeviceEvent,
    sink: &mut VirtualDevice,
    sink_dev_nodes: &[PathBuf],
    input_allow: &[PathBuf],
    input_deny: &[PathBuf],
    devices: &mut AHashMap<PathBuf, EventStream>,
    state: State,
) -> Result<State> {
    match event {
        DeviceEvent::Added(source_path) => {
            match add_device(
                source_path.clone(),
                sink_dev_nodes,
                input_allow,
                input_deny,
                devices,
            ) {
                Ok(Ok(source)) => {
                    if let Some(name) = source.name() {
                        info!("Tracking {name:?} ({source_path:?})");
                    } else {
                        info!("Tracking {source_path:?}");
                    }
                }
                Ok(Err(err)) => {
                    debug!("Rejected {source_path:?}: {err:#}");
                }
                Err(err) => {
                    warn!("Failed to track device {source_path:?}: {err:#}")
                }
            }
            Ok(state)
        }
        DeviceEvent::Removed(path) => Ok({
            if let Some(mut events) = devices.remove(&path) {
                if let Some(name) = events.device().name() {
                    info!("Untracking {name:?} ({path:?})");
                } else {
                    info!("Untracking {path:?}");
                }

                match state {
                    State::Swiping(swiping) if swiping.input_path == path => {
                        info!("Stopped swiping because the swipe device was removed");
                        swiping
                            // we never want to ungrab here, since the device is already removed
                            .stop(events.device_mut(), sink, false)
                            .with_context(|| "failed to stop swiping")?
                            .into()
                    }
                    state => state,
                }
            } else {
                state
            }
        }),
    }
}

fn on_input_event(
    swipe_2: Option<Key>,
    swipe_3: Option<Key>,
    swipe_4: Option<Key>,
    swipe_5: Option<Key>,
    x_mult: f32,
    y_mult: f32,
    grab: bool,
    source: &mut Device,
    source_path: &PathBuf,
    sink: &mut VirtualDevice,
    input: InputEvent,
    state: State,
) -> Result<State> {
    Ok(match state {
        State::Normal(normal) => {
            struct StartInfo {
                trigger: Key,
                fingers: Fingers,
            }

            let mut start_info = None;
            let mut test_start_swipe = |trigger: Option<Key>, fingers| {
                let Some(trigger) = trigger else { return };
                if input.kind() == InputEventKind::Key(trigger) && input.value() == 1 {
                    start_info = Some(StartInfo { trigger, fingers });
                }
            };

            test_start_swipe(swipe_2, Fingers::Two);
            test_start_swipe(swipe_3, Fingers::Three);
            test_start_swipe(swipe_4, Fingers::Four);
            test_start_swipe(swipe_5, Fingers::Five);

            if let Some(StartInfo { trigger, fingers }) = start_info {
                trace!("Started swipe on {source_path:?} with {fingers:?} fingers");
                normal
                    .start_swiping(source_path.clone(), source, sink, trigger, fingers, grab)
                    .with_context(|| "failed to start swiping")?
                    .into()
            } else {
                normal.into()
            }
        }
        State::Swiping(mut swiping) => match input.kind() {
            InputEventKind::RelAxis(RelativeAxisType::REL_X) => {
                swiping
                    .update(sink, input.value(), 0, x_mult, y_mult)
                    .with_context(|| "failed to update swipe position")?;
                swiping.into()
            }
            InputEventKind::RelAxis(RelativeAxisType::REL_Y) => {
                swiping
                    .update(sink, 0, input.value(), x_mult, y_mult)
                    .with_context(|| "failed to update swipe position")?;
                swiping.into()
            }
            InputEventKind::Key(key) if key == swiping.trigger && input.value() == 0 => {
                trace!("Stopped swipe on {source_path:?}");
                swiping
                    .stop(source, sink, grab)
                    .with_context(|| "failed to stop swiping")?
                    .into()
            }
            _ => swiping.into(),
        },
    })
}

fn add_device<'a>(
    source_path: PathBuf,
    sink_dev_nodes: &[PathBuf],
    input_allow: &[PathBuf],
    input_deny: &[PathBuf],
    devices: &'a mut AHashMap<PathBuf, EventStream>,
) -> Result<Result<&'a mut Device>> {
    const DEVICE_PREFIX: &str = "event";

    if sink_dev_nodes.contains(&source_path) {
        return Ok(Err(anyhow!("this is our own virtual device")));
    }

    if input_deny.contains(&source_path) {
        return Ok(Err(anyhow!("device is in the deny list")));
    }

    if !input_allow.is_empty() && !input_allow.contains(&source_path) {
        return Ok(Err(anyhow!("device is not in the allow list")));
    }

    let file_name = source_path
        .file_name()
        .with_context(|| "device path has no file name")?
        .to_str()
        .with_context(|| "file name is not UTF-8")?;
    if !file_name.starts_with(DEVICE_PREFIX) {
        return Ok(Err(anyhow!(
            "file name does not start with {DEVICE_PREFIX:?}"
        )));
    }

    let device = Device::open(&source_path).with_context(|| "failed to open device file")?;
    let Entry::Vacant(entry) = devices.entry(source_path) else {
        return Err(anyhow!("device with this file is already being tracked"));
    };
    let event_stream = device
        .into_event_stream()
        .with_context(|| "failed to convert device into event stream")?;
    let event_stream = entry.insert(event_stream);
    Ok(Ok(event_stream.device_mut()))
}

/*

async fn simulate_trackpad() -> Result<Never> {
    let mut sink_dev_nodes = Vec::new();
    let mut dev_nodes_iter = sink
        .enumerate_dev_nodes()
        .await
        .with_context(|| "failed to enumerate sink dev nodes")?;
    while let Ok(Some(dev_node)) = dev_nodes_iter.next_entry().await {
        sink_dev_nodes.push(dev_node);
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
                        match add_device(path.clone(), &sink_dev_nodes, input_allow, input_deny, &mut devices) {
                            Ok(Ok(device)) => {
                                if let Some(name) = device.name() {
                                    info!("Added {name:?} ({path:?})");
                                } else {
                                    info!("Added {path:?}");
                                }
                            }
                            Ok(Err(err)) => {
                                debug!("Rejected {path:?}: {err:#}");
                            }
                            Err(err) => {
                                warn!("Failed to add device {path:?}: {err:#}")
                            }
                        }
                    }
                    DeviceEvent::Removed(path) => {
                        if let Some(events) = devices.remove(&path) {
                            // if we're currently swiping with the device we're removing,
                            // stop swiping
                            if let State::Swiping { ref device, .. } = state {
                                if device == &path {
                                    stop_swipe();
                                }
                            }

                            if let Some(name) = events.device().name() {
                                info!("Removed {name:?} ({path:?})");
                            } else {
                                info!("Removed {path:?}");
                            }
                        }
                    }
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
 */
