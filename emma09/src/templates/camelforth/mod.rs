//! The CamelForth starter profile: Bradford J. Rodriguez's CamelForth for the
//! 6809, as ported to the Multicomp 6809 by Neal Crook and adapted to emma6k's
//! console device. The ROM image, its VICE labels, and the device layout are
//! embedded here; `forth/` holds the GPL-3.0-or-later source they're built
//! from, and `README.md` gives the license details and emma6k's changes.
use std::path::{Path, PathBuf};

use emma6k_core::config::template::asset::{self, MaterializeError};

const ROM_IMAGE: &[u8] = include_bytes!("program.bin");
const LABELS: &[u8] = include_bytes!("program.lbl");
const TEMPLATE: &str = include_str!("emulator-template.toml");

/// Writes the ROM image, labels file, and a rendered `emulator.toml` into
/// `dest` (created if missing). Returns the path to the `emulator.toml`.
pub fn materialize_config(dest: &Path) -> Result<PathBuf, MaterializeError> {
    asset::materialize(dest, ROM_IMAGE, LABELS, TEMPLATE)
}
