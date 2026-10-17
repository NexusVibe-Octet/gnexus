//! Macro recording (the RECORD button): what the platform layer captured
//! ([`Recorded`], see [`crate::platform::start_recording`]) turned into macro
//! steps in the profile file syntax (`down A`, `mouse_up Left`, `wait 120`…).

use std::collections::BTreeSet;
use std::time::Duration;

use crate::keycode::{KeyCode, MouseButton};

/// One input captured while recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Key(KeyCode, bool),
    Mouse(MouseButton, bool),
    /// Wheel notches (+1 = up).
    Wheel(i32),
}

/// Pause written between steps when the real delays are not kept.
pub const FIXED_DELAY_MS: u64 = 25;

/// How to turn a capture into steps.
#[derive(Debug, Clone, Copy)]
pub struct StepOptions {
    /// Keep the real delays between actions (otherwise [`FIXED_DELAY_MS`]).
    pub delays: bool,
    /// Drop the final left click (the click on STOP).
    pub drop_stop_click: bool,
}

/// Macro steps for `events` (time since the start of the recording, input).
/// Releases whose press was not recorded are dropped, and whatever is still
/// held at the end is released, so a recorded macro never leaves a key down.
pub fn to_steps(events: &[(Duration, Recorded)], o: StepOptions) -> Vec<String> {
    let mut ev: Vec<(Duration, Recorded)> = events.to_vec();
    if o.drop_stop_click {
        let up = ev.iter().rposition(|(_, e)| matches!(e, Recorded::Mouse(MouseButton::Left, false)));
        if let Some(u) = up {
            let down = ev[..u].iter().rposition(|(_, e)| matches!(e, Recorded::Mouse(MouseButton::Left, true)));
            // only when nothing but that click comes after it
            if let Some(d) = down
                && ev[d + 1..].iter().all(|(_, e)| matches!(e, Recorded::Mouse(MouseButton::Left, _)))
            {
                ev.truncate(d);
            }
        }
    }
    let mut keys: BTreeSet<KeyCode> = BTreeSet::new();
    let mut buttons: Vec<MouseButton> = Vec::new();
    let mut steps = Vec::new();
    let mut last: Option<Duration> = None;
    for (t, e) in ev {
        let step = match e {
            Recorded::Key(k, true) => keys.insert(k).then(|| format!("down {}", k.name())),
            Recorded::Key(k, false) => keys.remove(&k).then(|| format!("up {}", k.name())),
            Recorded::Mouse(b, true) => (!buttons.contains(&b)).then(|| {
                buttons.push(b);
                format!("mouse_down {}", b.name())
            }),
            Recorded::Mouse(b, false) => buttons.contains(&b).then(|| {
                buttons.retain(|x| *x != b);
                format!("mouse_up {}", b.name())
            }),
            Recorded::Wheel(n) if n != 0 => Some(format!("wheel {n}")),
            Recorded::Wheel(_) => None,
        };
        let Some(step) = step else { continue };
        if let Some(prev) = last {
            let ms = if o.delays { t.saturating_sub(prev).as_millis() as u64 } else { FIXED_DELAY_MS };
            if ms > 0 {
                steps.push(format!("wait {ms}"));
            }
        }
        last = Some(t);
        steps.push(step);
    }
    for k in keys.iter().rev() {
        steps.push(format!("up {}", k.name()));
    }
    for b in buttons.iter().rev() {
        steps.push(format!("mouse_up {}", b.name()));
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn keys_with_and_without_delays() {
        let ev = [
            (ms(0), Recorded::Key(KeyCode::LeftCtrl, true)),
            (ms(80), Recorded::Key(KeyCode::C, true)),
            (ms(80), Recorded::Key(KeyCode::C, true)), // auto-repeat
            (ms(150), Recorded::Key(KeyCode::C, false)),
            (ms(200), Recorded::Key(KeyCode::LeftCtrl, false)),
        ];
        let real = to_steps(&ev, StepOptions { delays: true, drop_stop_click: false });
        assert_eq!(real, ["down LeftCtrl", "wait 80", "down C", "wait 70", "up C", "wait 50", "up LeftCtrl"]);
        let fixed = to_steps(&ev, StepOptions { delays: false, drop_stop_click: false });
        assert_eq!(fixed[1], "wait 25");
    }

    /// The hooks start, and stop without hanging (real input only shows up by hand).
    #[test]
    #[cfg(windows)]
    fn hooks_start_and_stop() {
        let r = crate::platform::start_recording().expect("hooks");
        assert!(crate::platform::start_recording().is_err(), "one recording at a time");
        std::thread::sleep(ms(150));
        let _ = r.stop();
        let again = crate::platform::start_recording().expect("again after stop");
        let _ = again.stop();
    }

    #[test]
    fn stop_click_and_orphans() {
        let ev = [
            (ms(0), Recorded::Mouse(MouseButton::Left, false)), // end of the click on RECORD
            (ms(10), Recorded::Key(KeyCode::A, true)),
            (ms(40), Recorded::Mouse(MouseButton::Right, true)),
            (ms(90), Recorded::Wheel(-2)),
            (ms(500), Recorded::Mouse(MouseButton::Left, true)), // the click on STOP
            (ms(560), Recorded::Mouse(MouseButton::Left, false)),
        ];
        let s = to_steps(&ev, StepOptions { delays: true, drop_stop_click: true });
        // A and the right button are still held: released at the end
        assert_eq!(s, ["down A", "wait 30", "mouse_down Right", "wait 50", "wheel -2", "up A", "mouse_up Right"]);
    }
}
