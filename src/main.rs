#![doc = include_str!("../README.md")]

use std::{
    io, thread,
    time::{Duration, SystemTime},
};

use anyhow::{anyhow, Result};
use evdev_rs::{
    enums::{
        BusType, EventCode, InputProp,
        EV_ABS::{self, *},
        EV_KEY::{self, *},
        EV_REL::{self, *},
        EV_SYN::{self, *},
    },
    AbsInfo, DeviceWrapper, EnableCodeData, InputEvent, TimeVal, UInputDevice, UninitDevice,
};

// Copied from my evemu recording of my laptop trackpad
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

fn main() -> Result<()> {
    let dev = UninitDevice::new().ok_or(anyhow!("failed to create uninit device"))?;
    dev.set_name("fukomaster virtual trackpad");
    dev.set_bustype(BusType::BUS_USB as u16);

    // randomly generated IDs
    dev.set_vendor_id(0x4394);
    dev.set_product_id(0xd1fc);

    // https://www.kernel.org/doc/html/v4.12/input/event-codes.html
    // https://www.kernel.org/doc/html/v4.12/input/multi-touch-protocol.html

    dev.enable(InputProp::INPUT_PROP_POINTER)?;
    dev.enable(EventCode::EV_SYN(SYN_REPORT))?;

    enable_key(&dev, BTN_TOOL_FINGER)?;
    enable_key(&dev, BTN_TOUCH)?;
    enable_key(&dev, BTN_TOOL_DOUBLETAP)?;
    enable_key(&dev, BTN_TOOL_TRIPLETAP)?;
    enable_key(&dev, BTN_TOOL_QUADTAP)?;
    enable_key(&dev, BTN_TOOL_QUINTTAP)?;

    enable_abs(&dev, ABS_MT_SLOT, 4, 0)?; // max 5 touches
    enable_abs(&dev, ABS_MT_TRACKING_ID, 65535, 0)?;
    enable_abs(&dev, ABS_MT_TOOL_TYPE, 2, 0)?;
    enable_abs(&dev, ABS_MT_POSITION_X, 10000, 12)?;
    enable_abs(&dev, ABS_MT_POSITION_Y, 10000, 12)?;

    let dev = UInputDevice::create_from_device(&dev)?;

    eprintln!("Starting");
    thread::sleep(Duration::from_secs(3));
    eprintln!("Started");

    // copied from my evemu recording
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

    let start_x = 5000;
    emit(&dev, ABS_MT_TRACKING_ID, 8661)?;
    emit(&dev, ABS_MT_POSITION_X, start_x)?;
    emit(&dev, ABS_MT_POSITION_Y, 5000)?;

    emit(&dev, ABS_MT_SLOT, 1)?;
    emit(&dev, ABS_MT_TRACKING_ID, 8662)?;
    emit(&dev, ABS_MT_POSITION_X, start_x)?;
    emit(&dev, ABS_MT_POSITION_Y, 6000)?;

    emit(&dev, ABS_MT_SLOT, 2)?;
    emit(&dev, ABS_MT_TRACKING_ID, 8663)?;
    emit(&dev, ABS_MT_POSITION_X, start_x)?;
    emit(&dev, ABS_MT_POSITION_Y, 7000)?;

    emit(&dev, BTN_TOUCH, 1)?;
    emit(&dev, BTN_TOOL_TRIPLETAP, 1)?;
    // ABS_X, ABS_Y

    sync(&dev)?;

    for i in 0..100 {
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

        let x = start_x + i * 2;

        emit(&dev, ABS_MT_SLOT, 0)?;
        emit(&dev, ABS_MT_POSITION_X, x)?;
        emit(&dev, ABS_MT_SLOT, 1)?;
        emit(&dev, ABS_MT_POSITION_X, x)?;
        emit(&dev, ABS_MT_SLOT, 2)?;
        emit(&dev, ABS_MT_POSITION_X, x)?;
        // ABS_X
        sync(&dev)?;

        thread::sleep(Duration::from_millis(10));
    }

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

    emit(&dev, ABS_MT_SLOT, 1)?;
    emit(&dev, ABS_MT_TRACKING_ID, -1)?;
    emit(&dev, ABS_MT_SLOT, 2)?;
    emit(&dev, ABS_MT_TRACKING_ID, -1)?;
    emit(&dev, BTN_TOOL_FINGER, 1)?;
    emit(&dev, BTN_TOOL_TRIPLETAP, 0)?;
    sync(&dev)?;

    emit(&dev, ABS_MT_SLOT, 0)?;
    emit(&dev, ABS_MT_TRACKING_ID, -1)?;
    emit(&dev, BTN_TOUCH, 0)?;
    emit(&dev, BTN_TOOL_FINGER, 0)?;
    sync(&dev)?;

    Ok(())
}

trait IntoEventCode {
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

fn enable_key(dev: &UninitDevice, event: EV_KEY) -> Result<(), io::Error> {
    dev.enable(EventCode::EV_KEY(event))
}

fn enable_abs(
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

fn emit(dev: &UInputDevice, event: impl IntoEventCode, value: i32) -> Result<()> {
    let now = TimeVal::try_from(SystemTime::now())?;
    dev.write_event(&InputEvent {
        time: now,
        event_code: event.into_event_code(),
        value,
    })?;
    Ok(())
}

fn sync(dev: &UInputDevice) -> Result<()> {
    emit(dev, EV_SYN::SYN_REPORT, 0)
}
