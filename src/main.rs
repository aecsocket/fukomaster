#![doc = include_str!("../README.md")]

mod util;

use std::{fs::File, io, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use evdev_rs::{
    enums::{
        BusType, EventCode, InputProp,
        EV_ABS::*,
        EV_KEY::{self, *},
        EV_REL::*,
        EV_SYN::*,
    },
    Device, DeviceWrapper, GrabMode, ReadFlag, UInputDevice, UninitDevice,
};

use log::{debug, info, warn};
use util::{emit, enable_abs, enable_key, sync, Abs};

/// Simulate a trackpad with your physical mouse
#[derive(Debug, Clone, clap::Parser)]
struct Args {
    /// Source device file to read mouse inputs from (e.g. `/dev/input/event1`)
    #[arg(short, long)]
    source: PathBuf,
    /// `uinput` key code which activates swiping mode
    ///
    /// When this is pressed, the swiping mode is activated. When this is
    /// released, the swiping mode is deactivated.
    ///
    /// Use `wev` to test which button on your mouse you want to use for
    /// activation. For the MX Master 3S, the value is `277`.
    #[arg(short, long, default_value_t = 277)]
    trigger_key: u32,
    /// Number of fingers to simulate a swipe with
    #[arg(short, long, default_value_t = 3)]
    fingers: u8,
    /// Resolution of the virtual trackpad
    ///
    /// A larger resolution means you have to move your mouse further to have
    /// the trackpad move the same distance.
    ///
    /// The value is used directly as the resolution of the virtual `uinput`
    /// device.
    #[arg(short, long, default_value_t = 12)]
    resolution: u16,
    /// Swipe speed multiplier on the X axis
    #[arg(short, long, default_value_t = 1.0)]
    x_mult: f32,
    /// Swipe speed multiplier on the Y axis
    #[arg(short, long, default_value_t = 1.0)]
    y_mult: f32,
    /// Disables grabbing the mouse cursor in `evdev` when swiping
    ///
    /// If grabbing is disabled, the mouse cursor will move with the virtual
    /// trackpad when swiping, but may resolve issues with other processes
    /// which also attempt to grab the mouse.
    #[arg(long)]
    no_grab: bool,
    /// Enter the swiping mode when the process starts
    #[arg(long)]
    start_swiping: bool,
}

fn main() -> Result<()> {
    init_logging();

    let Args {
        source,
        trigger_key,
        fingers,
        resolution,
        x_mult,
        y_mult,
        no_grab,
        start_swiping,
    } = Args::parse();

    let trigger_key =
        evdev_rs::enums::int_to_ev_key(trigger_key).ok_or(anyhow!("invalid trigger key code"))?;

    let (fingers, btn_tool): (i32, _) = match fingers {
        2 => (2, BTN_TOOL_DOUBLETAP),
        3 => (3, BTN_TOOL_TRIPLETAP),
        4 => (4, BTN_TOOL_QUADTAP),
        5 => (5, BTN_TOOL_QUINTTAP),
        _ => {
            return Err(anyhow!("can only swipe with 2, 3, 4 or 5 fingers"));
        }
    };

    info!("Starting virtual trackpad sourced from {source:?}");

    let mut source = open_device(source).with_context(|| "failed to open source device")?;
    let sink =
        create_virtual_trackpad(resolution).with_context(|| "failed to create virtual trackpad")?;
    // we need a slight delay after creating the input device
    // so that other components can recognize it
    std::thread::sleep(std::time::Duration::from_millis(200));
    info!("Created virtual trackpad");
    info!("  devnode = {:?}", sink.devnode());
    info!("  syspath = {:?}", sink.syspath());

    simulate_trackpad(
        &mut source,
        &sink,
        Config {
            fingers,
            trigger_key,
            btn_tool,
            x_mult,
            y_mult,
            grab: !no_grab,
            start_swiping,
        },
    )?;

    Ok(())
}

fn init_logging() {
    let mut builder = pretty_env_logger::formatted_timed_builder();
    builder.filter_level(log::LevelFilter::Info);
    builder.parse_default_env();
    builder.init();
}

fn open_device(path: PathBuf) -> Result<Device> {
    // https://github.com/ndesh26/evdev-rs/blob/master/examples/vmouse.rs
    let file = File::open(path).with_context(|| "failed to open device file")?;
    Device::new_from_file(file).with_context(|| "failed to create device from file")
}

fn create_virtual_trackpad(resolution: u16) -> Result<UInputDevice> {
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

    let dev = UninitDevice::new().ok_or(anyhow!("failed to create virtual device"))?;
    dev.set_name("fukomaster virtual trackpad");
    dev.set_bustype(BusType::BUS_VIRTUAL as u16);

    // randomly generated IDs
    dev.set_vendor_id(0x4394);
    dev.set_product_id(0xd1fc);

    (|| {
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

        enable_abs(&dev, ABS_MT_SLOT, Abs::with_max(4))?; // max 5 touches
        enable_abs(&dev, ABS_MT_TRACKING_ID, Abs::with_max(i32::MAX))?;

        let resolution = i32::from(resolution);
        enable_abs(
            &dev,
            ABS_MT_POSITION_X,
            Abs::new(i32::MIN, i32::MAX, resolution),
        )?;
        enable_abs(
            &dev,
            ABS_MT_POSITION_Y,
            Abs::new(i32::MIN, i32::MAX, resolution),
        )?;

        Ok::<(), io::Error>(())
    })()
    .with_context(|| "failed to initialize device")?;

    let dev = UInputDevice::create_from_device(&dev)
        .with_context(|| "failed to create initialized device")?;
    Ok(dev)
}

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
}

fn start_swipe(
    source: &mut Device,
    sink: &UInputDevice,
    fingers: i32,
    btn_tool: EV_KEY,
    grab: bool,
) -> Result<()> {
    if grab && source.grab(GrabMode::Grab).is_err() {
        warn!("Failed to grab source device, will not start swiping for now");
        return Ok(());
    }

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

    for finger in 0..fingers {
        emit(sink, ABS_MT_SLOT, finger)?;
        emit(sink, ABS_MT_TRACKING_ID, finger)?;
        // (0, 0) is the center of the virtual trackpad
        emit(sink, ABS_MT_POSITION_X, 0)?;
        emit(sink, ABS_MT_POSITION_Y, 0)?;
    }
    emit(sink, BTN_TOUCH, 1)?;
    emit(sink, btn_tool, 1)?;
    sync(sink)?;

    debug!("Started swiping");
    Ok(())
}

fn update_position(
    dev: &UInputDevice,
    fingers: i32,
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

    for finger in 0..fingers {
        emit(dev, ABS_MT_SLOT, finger)?;
        emit(dev, ABS_MT_POSITION_X, x)?;
        emit(dev, ABS_MT_POSITION_Y, y)?;
    }
    sync(dev)?;

    Ok(())
}

fn stop_swipe(
    source: &mut Device,
    sink: &UInputDevice,
    fingers: i32,
    btn_tool: EV_KEY,
    grab: bool,
) -> Result<()> {
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

    for finger in 0..fingers {
        emit(sink, ABS_MT_SLOT, finger)?;
        emit(sink, ABS_MT_TRACKING_ID, -1)?;
    }
    emit(sink, BTN_TOOL_FINGER, 0)?;
    emit(sink, btn_tool, 0)?;
    sync(sink)?;

    if grab && source.grab(GrabMode::Ungrab).is_err() {
        warn!("Failed to ungrab source device, will still stop swiping");
    }

    debug!("Stopped swiping");
    Ok(())
}
