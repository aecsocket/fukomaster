use std::{ops::Mul, time::Duration};

use anyhow::Result;
use uinput::{
    event::{
        absolute::{self, Multi},
        controller::{Digi, Mouse},
        relative::{self, Position},
        Absolute, Controller, Relative,
    },
    Event,
};

/*
Device:           PIXA3854:00 093A:0274 Touchpad
Kernel:           /dev/input/event5
Group:            5
Seat:             seat0, default
Size:             111x73mm
Capabilities:     pointer gesture
Tap-to-click:     disabled
Tap-and-drag:     enabled
Tap drag lock:    disabled
Left-handed:      disabled
Nat.scrolling:    disabled
Middle emulation: disabled
Calibration:      n/a
Scroll methods:   *two-finger edge
Click methods:    *button-areas clickfinger
Disable-w-typing: enabled
Disable-w-trackpointing: enabled
Accel profiles:   flat *adaptive custom
Rotation:         n/a
 */

/*
 Device:           Logitech MX Master 3S
Kernel:           /dev/input/event12
Group:            8
Seat:             seat0, default
Capabilities:     pointer
Tap-to-click:     n/a
Tap-and-drag:     n/a
Tap drag lock:    n/a
Left-handed:      disabled
Nat.scrolling:    disabled
Middle emulation: disabled
Calibration:      n/a
Scroll methods:   button
Click methods:    none
Disable-w-typing: n/a
Disable-w-trackpointing: n/a
Accel profiles:   flat *adaptive custom
Rotation:         0.0
 */

fn main() -> Result<()> {
    let mut device = uinput::default()?
        .name("fukomaster virtual touchpad")?
        // .event(Absolute::Position(absolute::Position::X))?
        // .min(0)
        // .max(10000)
        // .event(Absolute::Position(absolute::Position::Y))?
        // .min(0)
        // .max(10000)
        .event(Multi::Slot)?
        .min(0)
        .max(3)
        .event(Multi::TrackingId)?
        .min(0)
        .max(65535)
        .event(Multi::PositionX)?
        .min(0)
        .max(10000)
        .event(Multi::PositionY)?
        .min(0)
        .max(10000)
        .event(Multi::ToolType)?
        .event(Digi::Touch)?
        // .event(Event::Controller(Controller::Mouse(Mouse::Left)))?
        // .event(Event::Relative(Relative::Position(relative::Position::X)))?
        // .event(Event::Relative(Relative::Position(relative::Position::Y)))?
        .create()?;

    // for _ in 1..10 {
    println!("finna start in 3");
    std::thread::sleep(Duration::from_secs(3));
    println!("started");

    // touch finger
    let start_x = 5000;
    device.send(Multi::Slot, 0)?;
    device.send(Multi::TrackingId, 0)?;
    device.send(Multi::PositionX, start_x)?;
    device.send(Multi::PositionY, 5000)?;
    device.send(Digi::Finger, 1)?;
    // device.send(Digi::Touch, 0)?;
    // device.send(Multi::ToolType, 0)?;
    device.synchronize()?;

    // move right
    for i in 0..100 {
        let x = start_x + i * 2;
        device.send(Multi::PositionX, x)?;
        device.synchronize()?;

        std::thread::sleep(Duration::from_millis(10));
    }

    // lift finger
    device.send(Multi::Slot, 0)?;
    device.send(Multi::TrackingId, -1)?;
    device.send(Digi::Finger, 0)?;
    // device.send(Digi::Touch, 0)?;
    device.synchronize()?;

    // // start tracking 3 fingers
    // let start_x = 5000;
    // device.send(Multi::Slot, 0)?;
    // device.send(Multi::TrackingId, 1)?;
    // device.send(Multi::PositionX, start_x)?;
    // device.send(Multi::PositionY, 1000)?;
    // device.send(Digi::Touch, 0)?;
    // device.send(Multi::ToolType, 0)?; // finger?
    // device.synchronize()?;

    // device.send(Multi::Slot, 1)?;
    // device.send(Multi::TrackingId, 2)?;
    // device.send(Multi::PositionX, start_x)?;
    // device.send(Multi::PositionY, 1500)?;
    // device.send(Digi::Touch, 0)?;
    // device.send(Multi::ToolType, 0)?; // finger?
    // device.synchronize()?;

    // device.send(Multi::Slot, 2)?;
    // device.send(Multi::TrackingId, 3)?;
    // device.send(Multi::PositionX, start_x)?;
    // device.send(Multi::PositionY, 2000)?;
    // device.send(Digi::Touch, 0)?;
    // device.send(Multi::ToolType, 0)?; // finger?
    // device.synchronize()?;

    // // move fingers to the right
    // for i in 0..10 {
    //     let x = start_x + i * 5;

    //     device.send(Multi::Slot, 0)?;
    //     device.send(Multi::PositionX, x)?;
    //     device.synchronize()?;

    //     device.send(Multi::Slot, 1)?;
    //     device.send(Multi::PositionX, x)?;
    //     device.synchronize()?;

    //     device.send(Multi::Slot, 2)?;
    //     device.send(Multi::PositionX, x)?;
    //     device.synchronize()?;

    //     std::thread::sleep(Duration::from_millis(100));
    // }

    // // lift fingers
    // device.send(Multi::Slot, 0)?;
    // device.send(Multi::TrackingId, -1)?;
    // device.send(Digi::Touch, 0)?;
    // device.synchronize()?;

    // device.send(Multi::Slot, 1)?;
    // device.send(Multi::TrackingId, -1)?;
    // device.send(Digi::Touch, 0)?;
    // device.synchronize()?;

    // device.send(Multi::Slot, 2)?;
    // device.send(Multi::TrackingId, -1)?;
    // device.send(Digi::Touch, 0)?;
    // device.synchronize()?;

    // // }

    // // loop {}

    Ok(())
}
