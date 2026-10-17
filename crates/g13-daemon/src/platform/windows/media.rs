//! Media back-end for Windows.
//!
//! * "Now playing" comes from the system media session (the media overlay of
//!   Windows: Spotify, browsers, the Media Player app…) through WinRT
//!   `GlobalSystemMediaTransportControlsSessionManager`.
//! * Transport control uses media virtual keys (`SendInput`), which every
//!   player understands.
//!
//! WinRT calls are blocking (`.join()`); the LCD calls `now_playing()` from its
//! own media thread, never from the engine thread.

use std::time::{SystemTime, UNIX_EPOCH};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};

use super::injector::SendInputInjector;
use crate::keycode::KeyCode;
use crate::platform::{
    InputInjector, MediaCommand, MediaInfo, MediaProvider, PlatformError, PlatformResult, PlaybackStatus,
};

/// 100-ns ticks between 1601-01-01 (WinRT `DateTime`) and 1970-01-01.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

#[derive(Debug, Default)]
pub struct WinMediaProvider {
    keys: SendInputInjector,
    manager: Option<SessionManager>,
}

impl WinMediaProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn manager(&mut self) -> PlatformResult<&SessionManager> {
        if self.manager.is_none() {
            let m = SessionManager::RequestAsync()
                .and_then(|op| op.join())
                .map_err(|e| PlatformError::Os(format!("media sessions: {e}")))?;
            self.manager = Some(m);
        }
        Ok(self.manager.as_ref().expect("just set"))
    }
}

fn os(e: windows::core::Error) -> PlatformError {
    PlatformError::Os(e.message().to_string())
}

fn read(session: &Session) -> windows::core::Result<MediaInfo> {
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let status = match session.GetPlaybackInfo()?.PlaybackStatus()? {
        Status::Playing => PlaybackStatus::Playing,
        Status::Paused => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    };
    let album = props.AlbumTitle()?.to_string();

    // position / duration (players that don't report it leave EndTime at 0)
    let (mut position_s, mut duration_s) = (None, None);
    if let Ok(tl) = session.GetTimelineProperties() {
        let end = tl.EndTime()?.Duration as f64 / 1e7;
        if end > 0.0 {
            let mut pos = tl.Position()?.Duration as f64 / 1e7;
            // the position is a snapshot taken at LastUpdatedTime: move it forward
            if status == PlaybackStatus::Playing {
                let updated = tl.LastUpdatedTime()?.UniversalTime;
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64 / 100)
                    + EPOCH_DIFF_TICKS;
                if updated > 0 && now > updated {
                    pos += (now - updated) as f64 / 1e7;
                }
            }
            position_s = Some(pos.min(end));
            duration_s = Some(end);
        }
    }

    Ok(MediaInfo {
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        album: (!album.is_empty()).then_some(album),
        status,
        position_s,
        duration_s,
        source: session.SourceAppUserModelId().ok().map(|s| s.to_string()),
    })
}

impl MediaProvider for WinMediaProvider {
    fn now_playing(&mut self) -> PlatformResult<Option<MediaInfo>> {
        let manager = self.manager()?;
        let session = match manager.GetCurrentSession() {
            Ok(s) => s,
            // no media session at all
            Err(_) => return Ok(None),
        };
        let info = read(&session).map_err(os)?;
        Ok((!info.title.is_empty() || !info.artist.is_empty()).then_some(info))
    }

    fn send(&mut self, cmd: MediaCommand) -> PlatformResult {
        let key = match cmd {
            MediaCommand::PlayPause => KeyCode::MediaPlayPause,
            MediaCommand::Next => KeyCode::MediaNext,
            MediaCommand::Previous => KeyCode::MediaPrevious,
            MediaCommand::Stop => KeyCode::MediaStop,
        };
        self.keys.key(key, true)?;
        self.keys.key(key, false)
    }
}

#[cfg(test)]
mod tests {
    /// Manual check: `cargo test -p g13-daemon now_playing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn now_playing() {
        let mut p = super::WinMediaProvider::new();
        eprintln!("{:#?}", super::MediaProvider::now_playing(&mut p));
    }
}
