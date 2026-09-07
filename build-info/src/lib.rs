//! Shared build-time metadata capture, called from each emma65 crate's `build.rs` (issue #623).
//!
//! [`emit`] captures the short git commit hash, a UTC build timestamp, and whether HEAD carries
//! an exact-match release tag for the calling crate — `<tag_prefix>X.Y.Z`, per the release
//! skill's tag-prefix table (`.claude/skills/release/reference.md`) — and publishes two
//! `cargo:rustc-env` vars for the calling crate to read back via `env!`:
//!
//! - `BUILD_VERSION` — always non-empty: `CARGO_PKG_VERSION` in a dev build, otherwise the same
//!   text as `BUILD_INFO_LINE`. Meant for a CLI's `--version` output.
//! - `BUILD_INFO_LINE` — empty in a dev build; otherwise `Build <hash> (<date>)`, or
//!   `Version <version> Build <hash> (<date>)` when HEAD carries the release tag. Meant for a
//!   GUI About panel, where dev vs. production must stay distinguishable by presence/absence.
//!
//! Both vars are computed once, in `build.rs`, using `CARGO_CFG_DEBUG_ASSERTIONS` (present iff
//! the crate being built has `debug_assertions` enabled) rather than a `#[cfg(debug_assertions)]`
//! split in the calling crate's own runtime code.
//!
//! Deliberately unprefixed (not `EMMA65_...`): `cargo:rustc-env` values leak into the real
//! process environment when a binary launches via `cargo run`/`cargo tauri dev`, and
//! `EMMA65_`-prefixed vars are picked up by the config loader as overrides (issue #621).

use std::env;
use std::process::Command;

/// Emits `BUILD_VERSION` and `BUILD_INFO_LINE` as `cargo:rustc-env` vars for the calling crate.
/// `tag_prefix` is that crate's release tag prefix (e.g. `"emma65-lcd-display-v"`).
pub fn emit(tag_prefix: &str) {
    let pkg_version = env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let is_debug = env::var("CARGO_CFG_DEBUG_ASSERTIONS").is_ok();

    let build_info_line = if is_debug {
        String::new()
    } else {
        build_info_line(tag_prefix, &pkg_version)
    };
    let build_version = if build_info_line.is_empty() {
        pkg_version
    } else {
        build_info_line.clone()
    };

    println!("cargo:rustc-env=BUILD_VERSION={build_version}");
    println!("cargo:rustc-env=BUILD_INFO_LINE={build_info_line}");
}

/// Formats `Build <hash> (<date>)`, prefixed with `Version <pkg_version> ` if HEAD carries an
/// exact-match release tag for `tag_prefix`.
fn build_info_line(tag_prefix: &str, pkg_version: &str) -> String {
    let git_hash = git_short_hash();
    let build_date = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
    let base = format!("Build {git_hash} ({build_date})");
    if has_exact_release_tag(tag_prefix) {
        format!("Version {pkg_version} {base}")
    } else {
        base
    }
}

/// The short git commit hash for HEAD, or `"unknown"` if `git` isn't available or the source
/// tree isn't a git checkout (e.g. a source tarball build), rather than failing the build.
fn git_short_hash() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|hash| hash.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// True if any tag matching `<tag_prefix>*` points directly at HEAD — i.e. this build is of a
/// tagged release commit, not just some commit reachable from a tag.
fn has_exact_release_tag(tag_prefix: &str) -> bool {
    Command::new("git")
        .args([
            "tag",
            "--points-at",
            "HEAD",
            "--list",
            &format!("{tag_prefix}*"),
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| !output.stdout.is_empty())
}
