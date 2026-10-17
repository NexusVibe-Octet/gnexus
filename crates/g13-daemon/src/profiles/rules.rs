//! Executable / window → profile association rules.

use serde::{Deserialize, Serialize};

use crate::platform::ForegroundApp;

/// One `[[rules]]` entry. Every field that is set must match.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchRule {
    /// Executable file name (`"eldenring.exe"`), case-insensitive; the
    /// `.exe` suffix is optional so one rule works on Windows and Linux.
    /// Also compared with the Wayland app-id / WM_CLASS on Linux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// Case-insensitive substring of the window title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl MatchRule {
    pub fn exe(name: &str) -> Self {
        Self { exe: Some(name.into()), title: None }
    }

    pub fn matches(&self, app: &ForegroundApp) -> bool {
        if self.exe.is_none() && self.title.is_none() {
            return false;
        }
        let exe_ok = self.exe.as_deref().is_none_or(|want| {
            let want = strip_exe(want);
            strip_exe(&app.process_name).eq_ignore_ascii_case(want)
                || app.app_id.as_deref().is_some_and(|id| id.eq_ignore_ascii_case(want))
        });
        let title_ok = self
            .title
            .as_deref()
            .is_none_or(|want| app.title.to_lowercase().contains(&want.to_lowercase()));
        exe_ok && title_ok
    }
}

fn strip_exe(name: &str) -> &str {
    match name.len().checked_sub(4) {
        Some(i) if name.is_char_boundary(i) && name[i..].eq_ignore_ascii_case(".exe") => &name[..i],
        _ => name,
    }
}

/// First candidate (in iteration order) with a rule matching `app`.
pub fn select<'a, I>(candidates: I, app: &ForegroundApp) -> Option<&'a str>
where
    I: IntoIterator<Item = (&'a str, &'a [MatchRule])>,
{
    candidates.into_iter().find(|(_, rules)| rules.iter().any(|r| r.matches(app))).map(|(id, _)| id)
}
