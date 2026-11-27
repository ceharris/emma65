//! The 6809's bundled "starter profile" templates: each is a ROM image (a
//! 6809 program), its VICE labels file, and a device-layout template.
//!
//! The template mechanism ([`Template`] and writing the assets out) is
//! CPU-neutral and lives in [`emma6k_core::config::template`]; this module
//! provides the 6809's entries.
mod camelforth;

pub use emma6k_core::config::template::Template;

/// Every bundled starter-profile template, in display order.
pub static TEMPLATES: &[Template] = &[Template::new(
    "camelforth",
    "CamelForth",
    "6809 CamelForth (GPL-3.0) with 56K RAM, 8K ROM, and a console; Ctrl+C interrupts a running word",
    camelforth::materialize_config,
)];

/// The template a run with no devices configured falls back to, if any.
pub const DEFAULT_TEMPLATE: Option<&str> = Some("camelforth");
