//! `emma09`: runs `emma6k` for the 6809 family. It supplies the family's
//! default CPU variant when the configuration names none, and rejects a
//! variant or profile from another family.

#[path = "../wrapper.rs"]
mod wrapper;

fn main() {
    std::process::exit(wrapper::exec_emma6k("emma09", "6809"));
}
