//! Virtual Xbox 360 controller through the ViGEmBus driver.
//!
//! Games see a real XInput pad. The driver must be installed separately
//! (ViGEmBus, github.com/nefarius/ViGEmBus); without it [`available`] says so
//! and gamepad updates fail with a clear error.

use std::sync::Mutex;

use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

use crate::platform::{GamepadState, PlatformError, PlatformResult};

fn err(e: vigem_client::Error) -> PlatformError {
    match e {
        vigem_client::Error::BusNotFound => {
            PlatformError::Os("pilote ViGEmBus introuvable (à installer pour le mode manette)".into())
        }
        other => PlatformError::Os(format!("ViGEmBus : {other}")),
    }
}

/// Is the ViGEmBus driver installed and reachable?
pub fn available() -> PlatformResult {
    Client::connect().map(drop).map_err(err)
}

/// One virtual controller, plugged in on the first update.
#[derive(Default)]
pub struct VigemPad {
    target: Mutex<Option<Xbox360Wired<Client>>>,
}

impl std::fmt::Debug for VigemPad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VigemPad")
    }
}

impl VigemPad {
    pub fn update(&self, st: &GamepadState) -> PlatformResult {
        let mut guard = self.target.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            let client = Client::connect().map_err(err)?;
            let mut t = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
            t.plugin().map_err(err)?;
            t.wait_ready().map_err(err)?;
            log::info!("virtual Xbox 360 controller plugged in");
            *guard = Some(t);
        }
        let report = XGamepad {
            buttons: XButtons(st.buttons),
            left_trigger: st.left_trigger,
            right_trigger: st.right_trigger,
            thumb_lx: st.left_x,
            thumb_ly: st.left_y,
            thumb_rx: st.right_x,
            thumb_ry: st.right_y,
        };
        let res = guard.as_mut().map_or(Ok(()), |t| t.update(&report));
        if let Err(e) = res {
            // e.g. the driver was restarted: plug a fresh controller next time
            *guard = None;
            return Err(err(e));
        }
        Ok(())
    }

    pub fn unplug(&self) -> PlatformResult {
        let mut guard = self.target.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(mut t) = guard.take() {
            t.unplug().map_err(err)?;
            log::info!("virtual Xbox 360 controller unplugged");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs ViGEmBus: `cargo test -p g13-daemon vigem -- --ignored`.
    #[test]
    #[ignore = "needs the ViGEmBus driver"]
    fn vigem_plug_update_unplug() {
        available().unwrap();
        let pad = VigemPad::default();
        pad.update(&GamepadState { left_y: 32767, buttons: 0x1000, ..GamepadState::default() }).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(4));
        pad.update(&GamepadState::default()).unwrap();
        pad.unplug().unwrap();
    }
}
