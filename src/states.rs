use std::path::PathBuf;

use anyhow::{Context, Result};
use evdev::{uinput::VirtualDevice, AbsoluteAxisType, Device, EventType, InputEvent, Key};

#[derive(Debug, Clone, Copy)]
pub enum Fingers {
    Two,
    Three,
    Four,
    Five,
}

impl Fingers {
    pub fn count(&self) -> u8 {
        match self {
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
        }
    }

    pub fn btn_tool(&self) -> Key {
        match self {
            Self::Two => Key::BTN_TOOL_DOUBLETAP,
            Self::Three => Key::BTN_TOOL_TRIPLETAP,
            Self::Four => Key::BTN_TOOL_QUADTAP,
            Self::Five => Key::BTN_TOOL_QUINTTAP,
        }
    }
}

fn abs_event(axis_type: AbsoluteAxisType, value: i32) -> InputEvent {
    InputEvent::new_now(EventType::ABSOLUTE, axis_type.0, value)
}

#[derive(Debug)]
pub enum State {
    Normal(Normal),
    Swiping(Swiping),
}

impl Default for State {
    fn default() -> Self {
        Self::Normal(Normal::new())
    }
}

#[derive(Debug)]
pub struct Normal(());

impl From<Normal> for State {
    fn from(value: Normal) -> Self {
        Self::Normal(value)
    }
}

impl Normal {
    pub fn new() -> Self {
        Self(())
    }

    pub fn start_swiping(
        self,
        source_path: PathBuf,
        source: &mut Device,
        sink: &mut VirtualDevice,
        trigger: Key,
        fingers: Fingers,
        grab: bool,
    ) -> Result<Swiping> {
        if grab {
            source
                .grab()
                .with_context(|| "failed to grab source device")?;
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

        let events = (0..i32::from(fingers.count()))
            .flat_map(|finger| {
                [
                    abs_event(AbsoluteAxisType::ABS_MT_SLOT, finger),
                    abs_event(AbsoluteAxisType::ABS_MT_TRACKING_ID, finger),
                    abs_event(AbsoluteAxisType::ABS_MT_POSITION_X, 0),
                    abs_event(AbsoluteAxisType::ABS_MT_POSITION_Y, 0),
                ]
            })
            .chain([
                InputEvent::new(EventType::KEY, Key::BTN_TOUCH.0, 1),
                InputEvent::new(EventType::KEY, fingers.btn_tool().0, 1),
            ]);
        sink.emit(&events.collect::<Vec<_>>())?;

        Ok(Swiping {
            input_path: source_path,
            fingers,
            trigger,
            x: 0,
            y: 0,
        })
    }
}

#[derive(Debug)]
pub struct Swiping {
    pub input_path: PathBuf,
    pub trigger: Key,
    pub fingers: Fingers,
    pub x: i32,
    pub y: i32,
}

impl From<Swiping> for State {
    fn from(value: Swiping) -> Self {
        Self::Swiping(value)
    }
}

impl Swiping {
    pub fn update(
        &mut self,
        sink: &mut VirtualDevice,
        dx: i32,
        dy: i32,
        x_mult: f32,
        y_mult: f32,
    ) -> Result<()> {
        self.x += dx;
        self.y += dy;

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
        let x = ((self.x as f32) * x_mult) as i32;
        #[allow(clippy::cast_precision_loss)]
        #[allow(clippy::cast_possible_truncation)]
        let y = ((self.y as f32) * y_mult) as i32;

        let events = (0..i32::from(self.fingers.count())).flat_map(|finger| {
            [
                abs_event(AbsoluteAxisType::ABS_MT_SLOT, finger),
                abs_event(AbsoluteAxisType::ABS_MT_POSITION_X, x),
                abs_event(AbsoluteAxisType::ABS_MT_POSITION_Y, y),
            ]
        });
        sink.emit(&events.collect::<Vec<_>>())?;

        Ok(())
    }

    pub fn stop(self, source: &mut Device, sink: &mut VirtualDevice, grab: bool) -> Result<Normal> {
        if grab {
            source
                .ungrab()
                .with_context(|| "failed to ungrab source device")?;
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

        let events = (0..i32::from(self.fingers.count()))
            .flat_map(|finger| {
                [
                    abs_event(AbsoluteAxisType::ABS_MT_SLOT, finger),
                    abs_event(AbsoluteAxisType::ABS_MT_TRACKING_ID, -1),
                ]
            })
            .chain([
                InputEvent::new_now(EventType::KEY, Key::BTN_TOOL_FINGER.0, 0),
                InputEvent::new_now(EventType::KEY, self.fingers.btn_tool().0, 0),
            ]);
        sink.emit(&events.collect::<Vec<_>>())?;

        Ok(Normal(()))
    }
}
