//! The Motorola 6809 for the emma6k emulator.
//!
//! - [`cpu`]: the CPU (registers, opcode tables, indexed addressing, ALU and
//!   condition codes, interrupts) and its
//!   [`Processor`](emma6k_core::Processor) implementation, which
//!   `emma6k_core::exec` runs and steps.
//! - [`config`]: the top-level [`Config`] schema, the 6809's
//!   device registry, and building a 6809 [`EmulatorSession`].
//! - [`templates`]: the 6809's bundled starter-profile templates.
//!
//! The 6809 uses only `emma6k-core`'s public API, so it lives in its own crate,
//! like every CPU family after the 6502.
pub mod config;
pub mod cpu;
pub mod templates;

pub use config::{Config, CpuVariantSpec, device_registry};
pub use cpu::{Cpu, CpuBuilder, InvalidOpcodePolicy, Registers};

/// [`emma6k_core::session::EmulatorSession`] for the 6809 [`Cpu`].
pub type EmulatorSession = emma6k_core::session::EmulatorSession<cpu::Cpu>;

#[cfg(test)]
mod exec_tests;
