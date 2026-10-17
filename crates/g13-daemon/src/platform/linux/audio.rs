//! Volume of a playback device through PipeWire's PulseAudio server (`pactl`,
//! package `libpulse`, always there with `pipewire-pulse`).
//!
//! `pactl -f json` turns every text holding a non-ASCII character into `null`
//! (the localized `description`, "Stéréo analogique"…), so the device is found by
//! its properties as well (`node.nick`, `device.description`, `device.product.name`),
//! which are the hardware's own names.

use std::process::Command;

use serde::Deserialize;

use crate::platform::{OutputVolume, PlatformError, PlatformResult};

#[derive(Debug, Deserialize)]
struct Channel {
    /// "40%"
    value_percent: String,
}

#[derive(Debug, Deserialize)]
struct Sink {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    volume: std::collections::BTreeMap<String, Channel>,
    #[serde(default)]
    properties: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Sink {
    fn prop(&self, key: &str) -> Option<&str> {
        self.properties.get(key).and_then(|v| v.as_str()).filter(|s| *s != "(null)")
    }

    /// The names the device goes by (for matching and display).
    fn names(&self) -> impl Iterator<Item = &str> {
        [self.prop("device.description"), self.prop("node.nick"), self.prop("device.product.name")]
            .into_iter()
            .chain([self.description.as_deref().filter(|s| *s != "(null)")])
            .flatten()
    }

    fn matches(&self, name: &str) -> bool {
        let want = name.to_lowercase();
        self.names().any(|n| n.to_lowercase().contains(&want))
    }

    /// The loudest channel, 0..100 (PulseAudio allows more than 100 %).
    fn percent(&self) -> u8 {
        self.volume
            .values()
            .filter_map(|c| c.value_percent.trim_end_matches('%').trim().parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            .min(100) as u8
    }
}

fn pactl(args: &[&str]) -> PlatformResult<Vec<u8>> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| PlatformError::Os(format!("pactl : {e} (installez pipewire-pulse)")))?;
    if !out.status.success() {
        return Err(PlatformError::Os(format!("pactl {} : {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(out.stdout)
}

fn sinks() -> PlatformResult<Vec<Sink>> {
    let json = pactl(&["-f", "json", "list", "sinks"])?;
    serde_json::from_slice(&json).map_err(|e| PlatformError::Os(format!("pactl : {e}")))
}

fn find(name: &str) -> PlatformResult<Option<Sink>> {
    Ok(sinks()?.into_iter().find(|s| s.matches(name)))
}

/// PipeWire `node.name` of the playback device whose name contains `name`.
pub fn node_name(name: &str) -> PlatformResult<Option<String>> {
    Ok(find(name)?.map(|s| s.name))
}

/// Volume of the playback device whose name contains `name` (`None`: no such device).
pub fn output_volume(name: &str) -> PlatformResult<Option<OutputVolume>> {
    Ok(find(name)?.map(|s| OutputVolume {
        device: s.names().next().unwrap_or(&s.name).to_string(),
        volume: s.percent(),
        muted: s.mute,
    }))
}

/// Set the volume (0..100) and / or the mute of that device.
pub fn set_output_volume(name: &str, volume: Option<u8>, mute: Option<bool>) -> PlatformResult {
    let sink = find(name)?.ok_or_else(|| PlatformError::Os(format!("sortie audio « {name} » introuvable")))?;
    if let Some(v) = volume {
        pactl(&["set-sink-volume", &sink.name, &format!("{}%", v.min(100))])?;
    }
    if let Some(m) = mute {
        pactl(&["set-sink-mute", &sink.name, if m { "1" } else { "0" }])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pactl_json_with_nulls() {
        let json = r#"[{"index":94,"name":"alsa_output.usb-Logitech_PRO_X_Wireless_Gaming_Headset-00.analog-stereo",
            "description":"(null)","mute":true,
            "volume":{"front-left":{"value":26214,"value_percent":"40%","db":"-23.88 dB"},
                      "front-right":{"value":27000,"value_percent":"41%","db":"-23 dB"}},
            "properties":{"node.nick":"PRO X Wireless Gaming Headset","device.profile.description":"(null)"}}]"#;
        let sinks: Vec<Sink> = serde_json::from_str(json).unwrap();
        let s = &sinks[0];
        assert!(s.matches("PRO X Wireless"));
        assert!(!s.matches("ES1371"));
        assert_eq!(s.percent(), 41);
        assert_eq!(s.names().next(), Some("PRO X Wireless Gaming Headset"));
        assert!(s.mute);
    }
}
