use std::{io, time::SystemTime};

use evdev_rs::{
    enums::{EventCode, EV_ABS, EV_KEY, EV_SYN},
    AbsInfo, DeviceWrapper, EnableCodeData, InputEvent, TimeVal, UInputDevice, UninitDevice,
};

pub trait IntoEventCode {
    fn into_event_code(self) -> EventCode;
}

impl IntoEventCode for EventCode {
    fn into_event_code(self) -> EventCode {
        self
    }
}

impl IntoEventCode for EV_KEY {
    fn into_event_code(self) -> EventCode {
        EventCode::EV_KEY(self)
    }
}

impl IntoEventCode for EV_ABS {
    fn into_event_code(self) -> EventCode {
        EventCode::EV_ABS(self)
    }
}

impl IntoEventCode for EV_SYN {
    fn into_event_code(self) -> EventCode {
        EventCode::EV_SYN(self)
    }
}

pub fn enable_key(dev: &UninitDevice, event: EV_KEY) -> Result<(), io::Error> {
    dev.enable(EventCode::EV_KEY(event))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Abs {
    pub min: i32,
    pub max: i32,
    pub resolution: i32,
}

impl Abs {
    pub fn new(min: i32, max: i32, resolution: i32) -> Self {
        Self {
            min,
            max,
            resolution,
        }
    }

    pub fn with_max(max: i32) -> Self {
        Self {
            max,
            ..Default::default()
        }
    }
}

pub fn enable_abs(dev: &UninitDevice, event: EV_ABS, abs: Abs) -> Result<(), io::Error> {
    let data = AbsInfo {
        value: 0,
        minimum: abs.min,
        maximum: abs.max,
        fuzz: 0,
        flat: 0,
        resolution: abs.resolution,
    };
    dev.enable_event_code(
        &EventCode::EV_ABS(event),
        Some(EnableCodeData::AbsInfo(data)),
    )
}

pub fn emit(dev: &UInputDevice, event: impl IntoEventCode, value: i32) -> Result<(), io::Error> {
    let now = TimeVal::try_from(SystemTime::now()).expect("invalid system time");
    dev.write_event(&InputEvent {
        time: now,
        event_code: event.into_event_code(),
        value,
    })?;
    Ok(())
}

pub fn sync(dev: &UInputDevice) -> Result<(), io::Error> {
    emit(dev, EV_SYN::SYN_REPORT, 0)
}
