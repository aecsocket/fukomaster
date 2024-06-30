use std::{thread, time::Duration};

use anyhow::Result;
use evdev::{
    uinput::VirtualDeviceBuilder, AttributeSet, EventType, InputEvent, RelativeAxisType,
    UinputAbsSetup,
};

fn main() -> Result<()> {
    let mut dev = VirtualDeviceBuilder::new()?
        .name("fukomaster virtual trackpad")
        .with_relative_axes(&AttributeSet::from_iter([
            RelativeAxisType::REL_X,
            RelativeAxisType::REL_Y,
            RelativeAxisType::REL_WHEEL,
            RelativeAxisType::REL_HWHEEL,
        ]))?
        .build()?;

    for path in dev.enumerate_dev_nodes_blocking()? {
        eprintln!("Available as {}", path?.display());
    }

    eprintln!("Starting...");
    thread::sleep(Duration::from_secs(3));
    eprintln!("Started");

    // println!("Waiting for Ctrl-C...");
    // loop {
    //     use MoveDirection::*;

    //     let ev = new_move_mouse_event(Up, 50);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Moved mouse up");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_move_mouse_event(Down, 50);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Moved mouse down");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_move_mouse_event(Left, 50);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Moved mouse left");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_move_mouse_event(Right, 50);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Moved mouse right");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_scroll_mouse_event(Up, 1);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Scrolled mouse up");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_scroll_mouse_event(Down, 1);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Scrolled mouse down");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_scroll_mouse_event(Left, 1);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Scrolled mouse left");
    //     thread::sleep(Duration::from_millis(100));

    //     let ev = new_scroll_mouse_event(Right, 1);
    //     dev.emit(&[ev]).unwrap();
    //     println!("Scrolled mouse right");
    //     thread::sleep(Duration::from_millis(100));
    // }

    for _ in 0..100 {
        let ev = InputEvent::new_now(EventType::RELATIVE, RelativeAxisType::REL_X.0, 2);
        dev.emit(&[ev])?;
        thread::sleep(Duration::from_millis(10));
    }

    Ok(())
}

enum MoveDirection {
    Up,
    Down,
    Left,
    Right,
}

fn new_move_mouse_event(direction: MoveDirection, distance: u16) -> InputEvent {
    let (axis, distance) = match direction {
        MoveDirection::Up => (RelativeAxisType::REL_Y, -i32::from(distance)),
        MoveDirection::Down => (RelativeAxisType::REL_Y, i32::from(distance)),
        MoveDirection::Left => (RelativeAxisType::REL_X, -i32::from(distance)),
        MoveDirection::Right => (RelativeAxisType::REL_X, i32::from(distance)),
    };
    InputEvent::new_now(EventType::RELATIVE, axis.0, distance)
}

fn new_scroll_mouse_event(direction: MoveDirection, distance: u16) -> InputEvent {
    let (axis, distance) = match direction {
        MoveDirection::Up => (RelativeAxisType::REL_WHEEL.0, i32::from(distance)),
        MoveDirection::Down => (RelativeAxisType::REL_WHEEL.0, -i32::from(distance)),
        MoveDirection::Left => (RelativeAxisType::REL_HWHEEL.0, -i32::from(distance)),
        MoveDirection::Right => (RelativeAxisType::REL_HWHEEL.0, i32::from(distance)),
    };
    InputEvent::new_now(EventType::RELATIVE, axis, distance)
}
