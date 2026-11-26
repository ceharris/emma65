//! The mechanism behind bundled "starter profile" templates: a [`Template`] is
//! a named, self-contained bundle of a ROM image, VICE labels file, and
//! device-layout template that can be written out as a ready-to-run profile.
//!
//! This module is CPU-neutral. Each CPU family provides its own list of
//! templates (for the 6502, [`crate::mos6502::templates::TEMPLATES`]), since a
//! template's ROM image is a program for that CPU. [`find_in`] and
//! [`materialize_from`] work over whichever list they are given.
pub mod asset;

use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

pub use asset::MaterializeError;

/// One bundled starter-profile template.
pub struct Template {
    /// Stable identifier used by the `emma65` binary's `--profile` flag and
    /// the debugger's New Profile template picker.
    pub id: &'static str,
    /// Human-readable name shown in the New Profile template picker.
    pub name: &'static str,
    /// One-line description shown alongside `name` in the picker.
    pub description: &'static str,
    materialize_fn: fn(&Path) -> Result<PathBuf, MaterializeError>,
}

impl Template {
    /// Creates a template whose assets are written by `materialize_fn`
    /// (typically a thin wrapper around [`asset::materialize`]).
    pub const fn new(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        materialize_fn: fn(&Path) -> Result<PathBuf, MaterializeError>,
    ) -> Self {
        Self {
            id,
            name,
            description,
            materialize_fn,
        }
    }

    /// Writes this template's bundled assets into `dest` (created if
    /// missing). Returns the path to the written `emulator.toml`.
    pub fn materialize(&self, dest: &Path) -> Result<PathBuf, MaterializeError> {
        (self.materialize_fn)(dest)
    }
}

/// Looks up the template with the given `id` in `templates`.
pub fn find_in<'a>(templates: &'a [Template], id: &str) -> Option<&'a Template> {
    templates.iter().find(|t| t.id == id)
}

/// Materializes the template `id` from `templates` into `dest` (created if
/// missing). Returns the path to the written `emulator.toml`.
pub fn materialize_from(
    templates: &[Template],
    id: &str,
    dest: &Path,
) -> Result<PathBuf, TemplateError> {
    find_in(templates, id)
        .ok_or_else(|| {
            TemplateError::Unknown(UnknownTemplateError {
                id: id.to_string(),
                available: templates.iter().map(|t| t.id).collect(),
            })
        })?
        .materialize(dest)
        .map_err(TemplateError::Materialize)
}

/// Names a template `id` that has no entry in the template list searched.
#[derive(Debug)]
pub struct UnknownTemplateError {
    /// The requested, unregistered template id.
    pub id: String,
    /// The ids that were available, listed in the error message.
    available: Vec<&'static str>,
}

impl Display for UnknownTemplateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown starter-profile template '{}' (available: {})",
            self.id,
            self.available.join(", ")
        )
    }
}

impl std::error::Error for UnknownTemplateError {}

/// An error materializing a starter-profile template: either its `id` names
/// no registered template, or writing its assets failed.
#[derive(Debug)]
pub enum TemplateError {
    /// `id` has no entry in the template list searched.
    Unknown(UnknownTemplateError),
    /// The template was found but its assets could not be written.
    Materialize(MaterializeError),
}

impl Display for TemplateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::Unknown(e) => Display::fmt(e, f),
            TemplateError::Materialize(e) => Display::fmt(e, f),
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TemplateError::Unknown(e) => Some(e),
            TemplateError::Materialize(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_marker(dest: &Path) -> Result<PathBuf, MaterializeError> {
        asset::materialize(dest, b"rom", b"labels", "rom = \"{{ROM_IMAGE}}\"\n")
    }

    static TOY_TEMPLATES: &[Template] = &[
        Template::new("one", "One", "first", write_marker),
        Template::new("two", "Two", "second", write_marker),
    ];

    fn temp_dest(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "emma6k-template-mechanism-test-{name}-{:?}",
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn find_in_finds_by_id() {
        assert_eq!(find_in(TOY_TEMPLATES, "two").map(|t| t.name), Some("Two"));
        assert!(find_in(TOY_TEMPLATES, "three").is_none());
    }

    #[test]
    fn materialize_from_writes_the_found_template() {
        let dest = temp_dest("found");
        let toml_path = materialize_from(TOY_TEMPLATES, "one", &dest).unwrap();
        assert_eq!(toml_path, dest.join("emulator.toml"));
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn unknown_id_error_lists_the_available_ids() {
        let dest = temp_dest("unknown");
        let err = materialize_from(TOY_TEMPLATES, "nope", &dest).unwrap_err();
        assert!(matches!(err, TemplateError::Unknown(_)));
        let msg = err.to_string();
        assert!(msg.contains("'nope'"), "{msg}");
        assert!(msg.contains("one, two"), "{msg}");
        let _ = std::fs::remove_dir_all(&dest);
    }
}
