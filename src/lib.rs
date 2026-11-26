pub mod emulator;
pub mod mos6502;
pub mod watch;

// The assembler and disassembler moved under `mos6502`; these keep the
// `emma65::assembler` and `emma65::disassembler` paths working.
pub use mos6502::{assembler, disassembler};

mod location;
mod text;

#[cfg(test)]
mod test_support;
