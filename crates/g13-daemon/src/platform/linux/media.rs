//! Media back-end for Linux: MPRIS over the session D-Bus (`zbus`).
//!
//! Every player (Spotify, browsers, VLC, mpv with its plugin…) publishes
//! `org.mpris.MediaPlayer2.<name>`. The one shown is, like Windows' "current
//! session": a playing one, else the last one shown, else a paused one, else any.
//! Commands go to that player (`PlayPause`, `Next`, `Previous`, `Stop`).
//!
//! Properties are read fresh at each call (no cache): `Position` is never signalled.
//! Called once a second from the LCD's media thread, never from the engine.

use std::collections::HashMap;

use zbus::blocking::{Connection, Proxy};
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;

use crate::platform::{MediaCommand, MediaInfo, MediaProvider, PlatformError, PlatformResult, PlaybackStatus};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

fn os(e: impl std::fmt::Display) -> PlatformError {
    PlatformError::Os(format!("MPRIS : {e}"))
}

#[derive(Default)]
pub struct MprisProvider {
    conn: Option<Connection>,
    /// Bus name of the player shown last (kept while it is paused).
    current: Option<String>,
}

impl std::fmt::Debug for MprisProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MprisProvider").field("current", &self.current).finish()
    }
}

impl MprisProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn conn(&mut self) -> PlatformResult<Connection> {
        if self.conn.is_none() {
            self.conn = Some(Connection::session().map_err(os)?);
        }
        Ok(self.conn.clone().expect("just set"))
    }

    fn players(&mut self) -> PlatformResult<Vec<String>> {
        let conn = self.conn()?;
        let dbus = zbus::blocking::fdo::DBusProxy::new(&conn).map_err(os)?;
        let names = match dbus.list_names() {
            Ok(n) => n,
            Err(e) => {
                self.conn = None; // the bus went away: reconnect next time
                return Err(os(e));
            }
        };
        let mut players: Vec<String> =
            names.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with(PREFIX)).collect();
        players.sort();
        Ok(players)
    }

    fn proxy(&mut self, name: &str) -> PlatformResult<Proxy<'static>> {
        let conn = self.conn()?;
        zbus::blocking::proxy::Builder::new(&conn)
            .destination(name.to_owned())
            .and_then(|b| b.path(PATH))
            .and_then(|b| b.interface(PLAYER))
            .map(|b| b.cache_properties(CacheProperties::No))
            .and_then(|b| b.build())
            .map_err(os)
    }

    fn status(&mut self, name: &str) -> Option<PlaybackStatus> {
        let s: String = self.proxy(name).ok()?.get_property("PlaybackStatus").ok()?;
        Some(match s.as_str() {
            "Playing" => PlaybackStatus::Playing,
            "Paused" => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        })
    }

    /// The player to show / control (see the module comment).
    fn pick(&mut self) -> PlatformResult<Option<(String, PlaybackStatus)>> {
        let players = self.players()?;
        let statuses: Vec<(String, PlaybackStatus)> =
            players.into_iter().filter_map(|p| self.status(&p).map(|s| (p, s))).collect();
        let by = |want: PlaybackStatus| statuses.iter().find(|(_, s)| *s == want).cloned();
        let last = self.current.as_ref().and_then(|c| statuses.iter().find(|(p, _)| p == c).cloned());
        let chosen = by(PlaybackStatus::Playing)
            .or(last)
            .or_else(|| by(PlaybackStatus::Paused))
            .or_else(|| statuses.first().cloned());
        self.current = chosen.as_ref().map(|(p, _)| p.clone());
        Ok(chosen)
    }
}

fn text(meta: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let v = meta.get(key)?;
    if let Ok(s) = String::try_from(v.try_clone().ok()?) {
        return Some(s);
    }
    // xesam:artist is a list of strings
    Vec::<String>::try_from(v.try_clone().ok()?).ok().map(|l| l.join(", "))
}

/// Microseconds (`mpris:length` is `x` or, for some players, `t`) → seconds.
fn micros(v: Option<&OwnedValue>) -> Option<f64> {
    let v = v?;
    let us = i64::try_from(v).ok().or_else(|| u64::try_from(v).ok().map(|u| u as i64))?;
    (us > 0).then_some(us as f64 / 1e6)
}

impl MediaProvider for MprisProvider {
    fn now_playing(&mut self) -> PlatformResult<Option<MediaInfo>> {
        let Some((name, status)) = self.pick()? else { return Ok(None) };
        let p = self.proxy(&name)?;
        let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").unwrap_or_default();
        let duration_s = micros(meta.get("mpris:length"));
        let position_s = duration_s
            .and(p.get_property::<i64>("Position").ok())
            .filter(|&us| us >= 0)
            .map(|us| (us as f64 / 1e6).min(duration_s.unwrap_or(f64::MAX)));
        let info = MediaInfo {
            title: text(&meta, "xesam:title").unwrap_or_default(),
            artist: text(&meta, "xesam:artist").unwrap_or_default(),
            album: text(&meta, "xesam:album").filter(|a| !a.is_empty()),
            status,
            position_s,
            duration_s,
            source: Some(name.trim_start_matches(PREFIX).to_string()),
        };
        Ok((!info.title.is_empty() || !info.artist.is_empty()).then_some(info))
    }

    fn send(&mut self, cmd: MediaCommand) -> PlatformResult {
        let Some((name, _)) = self.pick()? else {
            return Err(PlatformError::Os("aucun lecteur multimédia (MPRIS)".into()));
        };
        let method = match cmd {
            MediaCommand::PlayPause => "PlayPause",
            MediaCommand::Next => "Next",
            MediaCommand::Previous => "Previous",
            MediaCommand::Stop => "Stop",
        };
        self.proxy(&name)?.call::<_, _, ()>(method, &()).map_err(os)
    }
}
