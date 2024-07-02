use std::{collections::hash_map::Entry, path::PathBuf};

use ahash::AHashMap;
use anyhow::{anyhow, Context, Result};
use evdev::{
    uinput::VirtualDevice, Device, EventStream, InputEvent, InputEventKind, Key, RelativeAxisType,
};
use futures::{never::Never, stream::FuturesUnordered, StreamExt};
use log::{debug, info, trace, warn};
use tokio::sync::mpsc;

use crate::{
    states::{Fingers, State},
    DeviceEvent,
};

#[allow(clippy::too_many_arguments)]
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

        state = tokio::select! {
            Some(event) = device_events.recv() => {
                drop(input_events);
                on_device_event(
                    event,
                    sink,
                    &sink_dev_nodes,
                    input_allow,
                    input_deny,
                    &mut devices,
                    state,
                )?
            }
            Some((source_path, source, input)) = input_events.next() => {
                on_input_event(
                    swipe_2,
                    swipe_3,
                    swipe_4,
                    swipe_5,
                    x_mult,
                    y_mult,
                    grab,
                    source,
                    source_path,
                    sink,
                    input,
                    state,
                )?
            }
        };
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
                    warn!("Failed to track device {source_path:?}: {err:#}");
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

#[allow(clippy::too_many_arguments)]
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
    input: Result<InputEvent, std::io::Error>,
    state: State,
) -> Result<State> {
    if !source_path.exists() {
        // this device has been removed, but notify hasn't told us about it yet
        debug!("Received event from {source_path:?} which no longer exists");
        return Ok(state);
    }

    let input = match input {
        Ok(input) => input,
        Err(err) => {
            warn!(
                "Failed to read events from {source_path:?}: {:#}",
                anyhow::Error::new(err)
            );
            return Ok(state);
        }
    };

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
