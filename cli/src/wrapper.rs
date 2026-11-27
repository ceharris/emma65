//! The shared body of the `emma65` and `emma09` wrapper programs: each runs
//! `emma6k` with its CPU family fixed.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// Replaces this process with `emma6k`, passing `--family <family>` and then
/// this program's own arguments. `emma6k` sees `program` as its name, so its
/// usage and error messages say `emma65` or `emma09`. Returns only on
/// failure, with the exit status to use.
pub fn exec_emma6k(program: &str, family: &str) -> i32 {
    let Some(emma6k) = find_emma6k() else {
        eprintln!("{program}: can't find emma6k next to {program} or on PATH");
        return 127;
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let err = Command::new(&emma6k)
        .arg0(program)
        .arg("--family")
        .arg(family)
        .args(args)
        .exec();
    eprintln!("{program}: failed to run {}: {err}", emma6k.display());
    126
}

/// Finds the `emma6k` program: first next to this program, then on `PATH`.
fn find_emma6k() -> Option<PathBuf> {
    let name = format!("emma6k{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .filter(|path| path.is_file());
    beside.or_else(|| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(&name))
                .find(|path| path.is_file())
        })
    })
}
