//! Profile manager: TOML files, executable → profile rules, automatic
//! switching on focus change and hot reload.
//!
//! ```text
//!  <config>/profiles/*.toml ──store──▶ ProfileStore ──select(app)──▶ id
//!                                          ▲                         │
//!            ForegroundWatcher (4 Hz) ─────┘      ProfileManager ────┴─▶ Engine::set_profile
//! ```

pub mod format;
pub mod lgs;
pub mod manager;
pub mod rules;
pub mod store;

pub use format::{FileDto, ProfileDoc, dto_to_toml, from_dto, from_toml, to_dto, to_toml};
pub use manager::ProfileManager;
pub use rules::MatchRule;
pub use store::ProfileStore;

/// Invalid profile file (message includes the location when known).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError(pub String);

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileError {}

#[cfg(test)]
mod tests;
