//! Bus device configuration support.
mod console;
mod device;
mod display;
mod emulator;
mod finch;
mod lcd_display;
mod led_matrix;
mod lfsr;
pub mod loader;
mod mc6840;
mod mc6850;
mod palette;
mod path;
mod phoebe;
mod pic_finch;
mod r6551;
mod ram;
mod registry;
mod rom;
pub mod template;
mod transport;
mod via6522;
mod vireo;
mod write_policy;

pub use console::ConsoleModule;
pub use device::{DeviceModule, DeviceModuleError, DeviceSpec};
pub use display::CharDisplayModule;
pub use emulator::{BuildError, BuiltMachine, MachineSpec, UnmappedPolicySpec, build_machine};
pub use finch::FinchModule;
pub use lcd_display::LcdDisplayModule;
pub use led_matrix::LedMatrixModule;
pub use lfsr::LfsrModule;
pub use mc6840::Mc6840Module;
pub use mc6850::Mc6850Module;
pub use path::ExpandedPathBuf;
pub use phoebe::PhoebeModule;
pub use pic_finch::PicFinchModule;
pub use r6551::R6551Module;
pub use ram::RamModule;
pub use registry::{
    DeviceRegistry, DisplayFrameSlot, DisplayGeometry, DisplayGeometrySlot, InstantiationContext,
    LcdDisplayFrameSlot, LcdDisplayGeometry, LcdDisplayGeometrySlot, LedMatrixFrameSlot,
    LedMatrixGeometry, LedMatrixGeometrySlot, TransportSlot,
};
pub use rom::RomModule;
pub use transport::{TransportSpec, TransportSpecFormat};
pub use via6522::Via6522Module;
pub use vireo::VireoModule;

// --- 6502 re-exports ---
// The top-level `Config` schema (with its `cpu-variant` selector) and the
// bundled templates belong to the 6502; these keep their pre-split
// `emma65::emulator::config::*` paths working.
pub use crate::mos6502::config::{Config, CpuVariantSpec};
pub use crate::mos6502::templates;
