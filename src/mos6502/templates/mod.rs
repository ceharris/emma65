//! The 6502's bundled "starter profile" templates: each is a ROM image (a
//! 6502 program), its VICE labels file, and a device-layout template.
//!
//! The template mechanism ([`Template`], [`TemplateError`], and writing the
//! assets out) is CPU-neutral and lives in
//! [`emulator::config::template`](crate::emulator::config::template); this
//! module provides the 6502's entries. Adding a template is additive: create
//! `templates/<id>/` with its three asset files plus a `mod.rs` calling
//! [`asset::materialize`], then add one [`Template`] entry to [`TEMPLATES`]
//! below. No existing template's code or files change.
mod ehbasic;
mod lcd;
mod msbasic;
mod rain;
mod snake;
mod taliforth;

use std::path::{Path, PathBuf};

use crate::emulator::config::template;
pub use crate::emulator::config::template::{
    MaterializeError, Template, TemplateError, UnknownTemplateError, asset,
};

/// The template [`materialize_default`] writes, and the one the `emma65`
/// binary and the debugger fall back to when nothing else is configured.
const DEFAULT_TEMPLATE: &str = "taliforth";

/// Every bundled starter-profile template, in display order.
pub static TEMPLATES: &[Template] = &[
    Template::new(
        "taliforth",
        "TaliForth2",
        "Forth-2012 with 32K RAM, 32K ROM, VIA, and MC6850",
        taliforth::materialize_config,
    ),
    Template::new(
        "msbasic",
        "Microsoft BASIC",
        "BASIC with 48K RAM, 12K ROM, and VIA",
        msbasic::materialize_config,
    ),
    Template::new(
        "ehbasic",
        "EhBASIC",
        "Lee Davison's EhBASIC with 48K RAM, 12K ROM, and VIA",
        ehbasic::materialize_config,
    ),
    Template::new(
        "rain",
        "Digital Rain",
        "Digital Rain demo using memory-mapped display and LFSR",
        rain::materialize_config,
    ),
    Template::new(
        "lcd",
        "LCD Display",
        "HD44780 compatible display demo",
        lcd::materialize_config,
    ),
    Template::new(
        "snake",
        "Snake",
        "Classic Snake game using the built-in console and VIA",
        snake::materialize_config,
    ),
];

/// Looks up a bundled template by its stable `id`.
pub fn find(id: &str) -> Option<&'static Template> {
    template::find_in(TEMPLATES, id)
}

/// Materializes the default template's bundled assets into `dest` (created if missing).
/// Returns the path to the written `emulator.toml`.
pub fn materialize_default(dest: &Path) -> Result<PathBuf, TemplateError> {
    materialize(DEFAULT_TEMPLATE, dest)
}

/// Materializes template `id`'s bundled assets into `dest` (created if missing).
/// Returns the path to the written `emulator.toml`.
pub fn materialize(id: &str, dest: &Path) -> Result<PathBuf, TemplateError> {
    template::materialize_from(TEMPLATES, id, dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dest(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "emma65-templates-registry-test-{name}-{:?}",
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn find_returns_registered_templates() {
        assert!(find("msbasic").is_some());
        assert!(find("ehbasic").is_some());
        assert!(find("taliforth").is_some());
    }

    #[test]
    fn find_returns_none_for_unregistered_id() {
        assert!(find("nope").is_none());
    }

    #[test]
    fn materialize_unknown_id_names_it_in_the_error() {
        let dest = temp_dest("unknown-id");
        let err = materialize("nope", &dest).unwrap_err();
        assert!(matches!(err, TemplateError::Unknown(_)));
        assert!(err.to_string().contains("nope"));
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn materialize_msbasic_writes_its_emulator_toml() {
        let dest = temp_dest("msbasic");
        let toml_path = materialize("msbasic", &dest).unwrap();
        assert_eq!(toml_path, dest.join("emulator.toml"));
        let _ = std::fs::remove_dir_all(&dest);
    }
}
