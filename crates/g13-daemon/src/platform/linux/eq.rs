//! G Nexus's equalizer on Linux: a PipeWire filter chain in front of the headset's
//! output, the counterpart of the Windows audio effect (`g13-apo`).
//!
//! - Installed once (no root, no driver): a drop-in of PipeWire's own filter-chain
//!   daemon (`~/.config/pipewire/filter-chain.conf.d/gnexus-eq.conf`, user service
//!   `filter-chain.service`). Like the Windows effect, it keeps working when G Nexus
//!   is closed, and after a restart.
//! - A **smart filter** (WirePlumber 0.5): every stream going to the headset's output
//!   goes through it, without changing the default output.
//! - Graph: a gain for headroom (`bq_highshelf` at 0 Hz, [`EqSettings::preamp_db`]) then
//!   one `bq_peaking` per band, as `g13-dsp` computes them (same RBJ filters, same Q).
//! - New gains are applied live (`pw-cli set-param gnexus_eq Props`, no gap in the
//!   sound) and written to the drop-in for the next start.

use std::path::PathBuf;
use std::process::Command;

use g13_dsp::EqSettings;

use crate::platform::{PlatformError, PlatformResult};

/// `node.name` of the filter's sink side.
pub const NODE: &str = "gnexus_eq";
const SERVICE: &str = "filter-chain.service";

/// The drop-in read by PipeWire's filter-chain daemon.
pub fn conf_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("pipewire").join("filter-chain.conf.d").join("gnexus-eq.conf"))
}

/// (preamp, gain per band) actually applied: all 0 when the equalizer is off or flat.
fn gains(s: &EqSettings) -> (f32, Vec<f32>) {
    if s.is_flat() {
        (0.0, vec![0.0; s.bands.len()])
    } else {
        (s.preamp_db(), s.bands.iter().map(|&(_, g)| g).collect())
    }
}

/// SPA-JSON string (the target's `node.name` comes from PipeWire, but stay safe).
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The drop-in for `settings`, in front of the output whose `node.name` is `target`.
pub fn config(s: &EqSettings, target: &str) -> String {
    let (pre, g) = gains(s);
    let mut nodes = format!(
        "            {{ type = builtin name = pre label = bq_highshelf control = {{ Freq = 0.0 Q = 1.0 Gain = {pre:.1} }} }}\n"
    );
    let mut links = String::new();
    let mut prev = "pre".to_string();
    for (i, (&(freq, _), gain)) in s.bands.iter().zip(&g).enumerate() {
        let name = format!("b{}", i + 1);
        nodes += &format!(
            "            {{ type = builtin name = {name} label = bq_peaking control = {{ Freq = {freq:.1} Q = {q:.3} Gain = {gain:.1} }} }}\n",
            q = s.q
        );
        links += &format!("            {{ output = \"{prev}:Out\" input = \"{name}:In\" }}\n");
        prev = name;
    }
    format!(
        r#"# G Nexus : égaliseur du casque (écrit par G Nexus, ne pas modifier à la main).
context.modules = [
  {{ name = libpipewire-module-filter-chain
    args = {{
      node.description = "G Nexus — égaliseur"
      media.name       = "G Nexus — égaliseur"
      filter.graph = {{
        nodes = [
{nodes}        ]
        links = [
{links}        ]
      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props = {{
        node.name = "{NODE}"
        media.class = Audio/Sink
        filter.smart = true
        filter.smart.name = "gnexus-eq"
        filter.smart.target = {{ node.name = {target} }}
      }}
      playback.props = {{
        node.name = "{NODE}.output"
        node.passive = true
        stream.dont-remix = true
      }}
    }}
  }}
]
"#,
        target = quoted(target)
    )
}

/// The `node.name` the installed drop-in targets.
fn installed_target() -> Option<String> {
    let text = std::fs::read_to_string(conf_path()?).ok()?;
    let rest = text.split("filter.smart.target = { node.name = ").nth(1)?;
    let end = rest.find(" }")?;
    serde_json::from_str::<String>(&rest[..end]).ok()
}

fn run(cmd: &str, args: &[&str]) -> PlatformResult {
    let out = Command::new(cmd).args(args).output().map_err(|e| PlatformError::Os(format!("{cmd} : {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(PlatformError::Os(format!("{cmd} {} : {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())))
    }
}

fn write(s: &EqSettings, target: &str) -> PlatformResult {
    let path = conf_path().ok_or_else(|| PlatformError::Os("pas de dossier personnel".into()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| PlatformError::Os(format!("{} : {e}", dir.display())))?;
    }
    std::fs::write(&path, config(s, target)).map_err(|e| PlatformError::Os(format!("{} : {e}", path.display())))
}

/// Is the equalizer installed (in front of any output)?
pub fn installed() -> bool {
    conf_path().is_some_and(|p| p.exists())
}

/// Install it in front of the output whose `node.name` is `target`, with `s`, and
/// (re)start PipeWire's filter-chain daemon (enabled for the next sessions too).
pub fn install(s: &EqSettings, target: &str) -> PlatformResult {
    write(s, target)?;
    run("systemctl", &["--user", "enable", SERVICE])?;
    run("systemctl", &["--user", "restart", SERVICE])
}

/// Remove it (the filter-chain daemon is restarted without it). Never `disable` the
/// service: its `[Install]` has `Also=pipewire.socket`, which `disable` would drop too.
pub fn uninstall() -> PlatformResult {
    if let Some(p) = conf_path().filter(|p| p.exists()) {
        std::fs::remove_file(&p).map_err(|e| PlatformError::Os(format!("{} : {e}", p.display())))?;
    }
    run("systemctl", &["--user", "restart", SERVICE])
}

/// New settings: applied live to the running filter, and kept for its next start.
/// Nothing to do when the equalizer is not installed.
pub fn apply(s: &EqSettings) -> PlatformResult {
    let Some(target) = installed_target() else { return Ok(()) };
    write(s, &target)?;
    let (pre, g) = gains(s);
    let mut params = format!("\"pre:Gain\" {pre:.1}");
    for (i, gain) in g.iter().enumerate() {
        params += &format!(" \"b{}:Gain\" {gain:.1}", i + 1);
    }
    run("pw-cli", &["set-param", NODE, "Props", &format!("{{ params = [ {params} ] }}")])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_has_every_band_chained_after_the_preamp() {
        let mut s = EqSettings::default();
        s.bands[5].1 = 6.0; // 1 kHz
        s.bands[0].1 = -3.0;
        let c = config(&s, "alsa_output.usb-Logitech_PRO_X");
        assert!(c.contains("name = pre label = bq_highshelf control = { Freq = 0.0 Q = 1.0 Gain = -6.0 }"));
        assert!(c.contains("name = b6 label = bq_peaking control = { Freq = 1000.0 Q = 1.410 Gain = 6.0 }"));
        assert!(c.contains("name = b1 label = bq_peaking control = { Freq = 32.0 Q = 1.410 Gain = -3.0 }"));
        assert!(c.contains("{ output = \"pre:Out\" input = \"b1:In\" }"));
        assert!(c.contains("{ output = \"b9:Out\" input = \"b10:In\" }"));
        assert!(c.contains("filter.smart.target = { node.name = \"alsa_output.usb-Logitech_PRO_X\" }"));
    }

    #[test]
    fn off_means_untouched() {
        let mut s = EqSettings::default();
        s.bands[2].1 = 9.0;
        s.enabled = false;
        let (pre, g) = gains(&s);
        assert_eq!(pre, 0.0);
        assert!(g.iter().all(|&x| x == 0.0));
    }
}
