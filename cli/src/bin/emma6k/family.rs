//! CPU families: mapping a `cpu-variant` value to its family, and each
//! family's wrapper name, default variant, and bundled templates.
//!
//! `emma6k` only maps a variant to its family here. The family's own crate
//! parses the variant when the session is built, so a value the family
//! knows but doesn't support yet (such as `6309`) gets that crate's message.

use clap::ValueEnum;
use emma6k_core::config::template::Template;
use std::fmt::{Display, Formatter};

/// A CPU family: the CPUs one crate emulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Family {
    /// The 65C02 and WDC 65C02 (`emma65`).
    #[value(name = "6502")]
    Mos6502,
    /// The 6809 (`emma09`).
    #[value(name = "6809")]
    Mc6809,
}

/// Every family, in the order templates are listed.
pub const FAMILIES: [Family; 2] = [Family::Mos6502, Family::Mc6809];

impl Family {
    /// Returns the family `variant` belongs to. Case-insensitive.
    pub fn of(variant: &str) -> Result<Family, String> {
        match variant.to_ascii_uppercase().as_str() {
            "65C02" | "C02" | "WDC65C02" | "WDC02" => Ok(Family::Mos6502),
            "6809" | "6309" => Ok(Family::Mc6809),
            _ => Err(format!(
                "Invalid CPU variant '{variant}' (expected 65c02, wdc65c02 or 6809)"
            )),
        }
    }

    /// The variant used when nothing names one.
    pub fn default_variant(self) -> &'static str {
        match self {
            Family::Mos6502 => "65c02",
            Family::Mc6809 => "6809",
        }
    }

    /// The wrapper program that runs this family.
    pub fn wrapper(self) -> &'static str {
        match self {
            Family::Mos6502 => "emma65",
            Family::Mc6809 => "emma09",
        }
    }

    /// The family's bundled starter-profile templates.
    pub fn templates(self) -> &'static [Template] {
        match self {
            Family::Mos6502 => emma65::emulator::config::templates::TEMPLATES,
            Family::Mc6809 => emma09::templates::TEMPLATES,
        }
    }

    /// The template a run with no devices configured falls back to, if the
    /// family has one.
    pub fn default_template(self) -> Option<&'static str> {
        match self {
            Family::Mos6502 => Some(emma65::emulator::config::templates::DEFAULT_TEMPLATE),
            Family::Mc6809 => emma09::templates::DEFAULT_TEMPLATE,
        }
    }
}

impl Display for Family {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Family::Mos6502 => write!(f, "6502"),
            Family::Mc6809 => write!(f, "6809"),
        }
    }
}

/// Looks up a bundled template by id across every family's list. Returns the
/// template and its family, or an error listing every id.
pub fn find_template(id: &str) -> Result<(Family, &'static Template), String> {
    FAMILIES
        .iter()
        .find_map(|&family| {
            family
                .templates()
                .iter()
                .find(|t| t.id == id)
                .map(|t| (family, t))
        })
        .ok_or_else(|| {
            let available: Vec<&str> = FAMILIES
                .iter()
                .flat_map(|f| f.templates().iter().map(|t| t.id))
                .collect();
            format!(
                "unknown starter-profile template '{id}' (available: {})",
                available.join(", ")
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn variants_map_to_their_families() {
        for v in ["65c02", "C02", "wdc65c02", "WDC02"] {
            assert_eq!(Family::of(v), Ok(Family::Mos6502), "{v}");
        }
        for v in ["6809", "6309"] {
            assert_eq!(Family::of(v), Ok(Family::Mc6809), "{v}");
        }
        assert!(Family::of("z80").unwrap_err().contains("'z80'"));
    }

    #[test]
    fn template_ids_are_unique_across_families() {
        let mut seen = HashSet::new();
        for family in FAMILIES {
            for t in family.templates() {
                assert!(seen.insert(t.id), "duplicate template id '{}'", t.id);
            }
        }
    }

    #[test]
    fn each_default_template_is_in_its_family() {
        for family in FAMILIES {
            if let Some(id) = family.default_template() {
                assert_eq!(find_template(id).unwrap().0, family);
            }
        }
    }

    #[test]
    fn default_variants_are_in_their_families() {
        for family in FAMILIES {
            assert_eq!(Family::of(family.default_variant()), Ok(family));
        }
    }
}
