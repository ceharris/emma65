//! Emulator session configuration.
use super::ErrorReceiver;
use super::bus::DeviceIdAllocator;

/// Emulator session for use in a CLI or UI context: a fully configured CPU
/// plus the channels a host needs around it. `P` is the CPU type; for the
/// 6502 it is named `emma65::mos6502::EmulatorSession` (also re-exported as
/// `emma65::emulator::EmulatorSession`).
pub struct EmulatorSession<P> {
    /// Fully configured CPU for the session.
    pub cpu: P,
    /// Receiver for errors emitted by machine components.
    pub error_receiver: ErrorReceiver,
    /// Device ID allocator, left in its post-configuration state so callers
    /// (e.g. the debugger UI) can allocate additional IDs — such as an IRQ
    /// source for a UI-driven control — guaranteed not to collide with any
    /// configured device.
    pub id_allocator: DeviceIdAllocator,
}
