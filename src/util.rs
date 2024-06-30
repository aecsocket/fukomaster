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

pub fn enable_abs(
    dev: &UninitDevice,
    event: EV_ABS,
    maximum: i32,
    resolution: i32,
) -> Result<(), io::Error> {
    let data = AbsInfo {
        value: 0,
        minimum: 0,
        maximum,
        fuzz: 0,
        flat: 0,
        resolution,
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
