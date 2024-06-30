use std::{thread, time::Duration};

use anyhow::Result;
use evdev_rs::enums::InputProp;
use uinput::event::{
    absolute::Multi::{PositionX, PositionY, Slot, ToolType, TrackingId},
    controller::Digi::{self, Finger, Touch},
};

/// Maximum number of fingers which can be touching at the same time.
const MAX_TOUCHES: i32 = 3;

/// Size of the virtual trackpad.
const SIZE: i32 = 10_000 as i32;

fn main() -> Result<()> {
    let mut dev = uinput::default()?
        .name("fukomaster virtual touchpad")?
        .bus(3) // BUS_USB
        // randomly generated IDs
        .vendor(0x4394)
        .product(0xd1fc)
        // .event(Controller::Mouse(Mouse::Left))?
        // .event(Relative::Position(X))?
        // .event(Relative::Position(Y))?
        .event(Slot)?
        .min(0)
        .max(3)
        .event(TrackingId)?
        .min(0)
        .max(65535)
        .event(PositionX)?
        .min(0)
        .max(10000)
        .event(PositionY)?
        .min(0)
        .max(10000)
        .event(Digi::TripleTap)?
        // .event(ToolType)?
        // .event(Finger)? // commenting this out causes stuff to happen
        .event(Touch)?
        .create()?;

    eprintln!("Starting...");
    thread::sleep(Duration::from_secs(3));
    eprintln!("Started");

    // Start touch
    dev.send(Slot, 0)?;
    dev.send(TrackingId, 0)?;
    dev.send(PositionX, 5000)?;
    dev.send(PositionY, 9000)?;
    dev.send(Digi::TripleTap, 1)?;
    // dev.send(Touch, 1)?;
    dev.synchronize()?;

    // dev.send(Slot, 1)?;
    // dev.send(TrackingId, 1)?;
    // dev.send(PositionX, SIZE / 2)?;
    // dev.send(PositionY, SIZE / 2)?;
    // dev.send(Finger, 1)?;
    // dev.send(Touch, 1)?;
    // dev.synchronize()?;

    for _ in 0..100 {
        // dev.send(Slot, 0)?;
        dev.send(PositionX, 5000)?;
        dev.synchronize()?;

        // dev.send(Slot, 1)?;
        // dev.send(PositionX, 0)?;
        // dev.synchronize()?;

        // dev.send(Relative::Position(X), 3)?;
        // dev.synchronize()?;

        thread::sleep(Duration::from_millis(10));
    }

    dev.send(Slot, 0)?;
    dev.send(TrackingId, -1)?;
    dev.send(Digi::TripleTap, 0)?;
    // dev.send(Touch, 0)?;
    dev.synchronize()?;

    // dev.send(Slot, 1)?;
    // dev.send(TrackingId, -1)?;
    // dev.send(Finger, 0)?;
    // dev.send(Touch, 0)?;
    // dev.synchronize()?;

    eprintln!("Done");

    Ok(())
}
