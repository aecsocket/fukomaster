use std::{collections::hash_map::Entry, path::PathBuf, time::Duration};

use ahash::AHashMap;
use anyhow::{anyhow, Context, Result};
use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AbsInfo, AbsoluteAxisType, AttributeSet, Device, EventStream, InputEvent, InputEventKind, Key,
    PropType, RelativeAxisType, UinputAbsSetup,
};
use futures::{never::Never, stream::FuturesUnordered, StreamExt};
use log::{debug, info, trace, warn};
use tokio::sync::mpsc;

use crate::{
    states::{Fingers, State},
    NotifyEvent,
};

#[allow(clippy::too_many_arguments)]
pub async fn simulate(
    device_events: &mut mpsc::UnboundedReceiver<NotifyEvent>,
    input_allow: &[PathBuf],
    input_deny: &[PathBuf],
    swipe_2: Option<Key>,
    swipe_3: Option<Key>,
    swipe_4: Option<Key>,
    swipe_5: Option<Key>,
    resolution: u16,
    x_mult: f32,
    y_mult: f32,
    grab: bool,
) -> Result<Never> {
    info!("Creating virtual trackpad");
    let (mut sink, sink_dev_nodes) = create_trackpad(resolution).await?;
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
                    &mut sink,
                    &sink_dev_nodes,
                    input_allow,
                    input_deny,
                    &mut devices,
                    state
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
                    &mut sink,
                    input,
                    state,
                )?
            }
        };
    }
}

async fn create_trackpad(resolution: u16) -> Result<(VirtualDevice, Vec<PathBuf>)> {
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

    const VIRTUAL_DEVICE_NAME: &str = "fukomaster virtual trackpad";

    fn abs(min: i32, max: i32, resolution: i32) -> AbsInfo {
        AbsInfo::new(0, min, max, 0, 0, resolution)
    }

    fn abs_with_max(max: i32) -> AbsInfo {
        abs(0, max, 0)
    }

    let resolution = i32::from(resolution);
    let mut dev = VirtualDeviceBuilder::new()?
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

    // we need a slight delay after creating the input device
    // so that other processes (i.e. compositor) can recognize it
    tokio::time::sleep(Duration::from_millis(200)).await;

    info!("Created virtual trackpad");

    let dev_nodes = collect_dev_nodes(&mut dev)
        .await
        .with_context(|| "failed to enumerate dev nodes of device")?;
    let sys_path = dev
        .get_syspath()
        .with_context(|| "failed to get sys path of device")?;
    info!("  sys path = {sys_path:?}");
    for dev_node in &dev_nodes {
        info!("  dev node = {dev_node:?}");
    }

    Ok((dev, dev_nodes))
}

async fn collect_dev_nodes(device: &mut VirtualDevice) -> Result<Vec<PathBuf>> {
    let mut iter = device.enumerate_dev_nodes().await?;
    let mut nodes = Vec::new();
    while let Ok(Some(node)) = iter.next_entry().await {
        nodes.push(node);
    }
    Ok(nodes)
}

fn on_device_event(
    event: NotifyEvent,
    sink: &mut VirtualDevice,
    sink_dev_nodes: &[PathBuf],
    input_allow: &[PathBuf],
    input_deny: &[PathBuf],
    devices: &mut AHashMap<PathBuf, EventStream>,
    state: State,
) -> Result<State> {
    match event {
        NotifyEvent::Created(source_path) => {
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
                    debug!("Will not track {source_path:?}: {err:#}");
                }
                Err(err) => {
                    warn!("Failed to track device {source_path:?}: {err:#}");
                }
            }
            Ok(state)
        }
        NotifyEvent::Removed(path) => Ok({
            let Some(mut events) = devices.remove(&path) else {
                return Ok(state);
            };

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
