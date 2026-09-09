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
pub mod templates;
mod transport;
mod via6522;
mod vireo;
mod write_policy;

pub use console::ConsoleModule;
pub use device::{DeviceModule, DeviceModuleError, DeviceSpec};
pub use display::CharDisplayModule;
pub use emulator::{BuildError, Config, CpuVariantSpec};
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
