//! Interrupt controller.
//!
//! The interrupt controller handles signaling and recognition of the CPU's interrupt
//! inputs: RESET, NMI, IRQ, and (on CPUs that have it, such as the 6809) FIRQ.
//!
//! Tne interrupt controller plays a crucial role in identifying interrupt sources
//! during CPU step execution. As such the [`poll_devices`](InterruptController::poll_devices)
//! method, which is called for each instruction executed, must be especially
//! efficient. This implementation uses a bit set (in a `u64`) to track devices that
//! are currently asserting IRQ. `poll_devices` also latches any pending NMI edges and
//! device-initiated RESET requests reported by devices in the same pass, so the CPU only
//! needs to make one call (and the bus only needs to make one pass over its devices) per
//! instruction.
//!
//! To emulate the level-triggered nature of the CPU's IRQ signal, devices assert or
//! release IRQ (based on their internal state) using [`assert_irq`](InterruptController::assert_irq)
//! and [`release_irq`](InterruptController::release_irq), respectively. These methods simply
//! set or clear a bit in the bit set representing active IRQ sources. The CPU's IRQ signal
//! reads as asserted whenever the bit set is non-empty (i.e. the underlying `u64` is not zero).
//!
//! A source that wants "trigger exactly one IRQ" semantics (rather than a sustained
//! level requiring an explicit release) can use
//! [`assert_irq_pulse`](InterruptController::assert_irq_pulse) instead of `assert_irq`.
//! The CPU calls [`consume_irq_pulses`](InterruptController::consume_irq_pulses) once an
//! IRQ has been serviced, which releases any sources currently asserted in pulse mode.
//!
//! ## FIRQ routing
//!
//! Which maskable input a device drives is board wiring, not a property of the device, so
//! devices only ever report a single "interrupt requested" level. The configuration
//! framework routes each device to IRQ or FIRQ (the `interrupt` device attribute) and
//! records the FIRQ-routed devices in a fixed mask, set once with
//! [`set_firq_mask`](InterruptController::set_firq_mask). A FIRQ-routed source sets its bit
//! in the same source set as every other device, and the mask splits the set into the two
//! lines: [`irq_active`](InterruptController::irq_active) and
//! [`active_sources_mask`](InterruptController::active_sources_mask) see only the sources
//! outside the mask, and [`firq_active`](InterruptController::firq_active) only those inside
//! it. So polling costs the same whichever line a device is wired to.
//!
use crate::device::DeviceId;

/// Identifies a source of IRQ, mapped from the `DeviceId` of the asserting device.
///
/// Ordered by priority: a lower value is a higher-priority source (`IrqSource(0)` is
/// highest), matching real PIC hardware conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IrqSource(pub u32);

impl From<DeviceId> for IrqSource {
    fn from(id: DeviceId) -> Self {
        IrqSource(id.0)
    }
}

/// Maximum number of IRQ sources
pub const MAX_IRQ_SOURCES: u32 = 64;

/// One device's per-step interrupt state, as reported by
/// [`Bus::device_interrupt_states`](crate::bus::Bus::device_interrupt_states) and
/// consumed by [`InterruptController::poll_devices`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInterruptState {
    /// The device's ID on the bus.
    pub id: DeviceId,
    /// `true` if the device is currently asserting IRQ.
    pub irq_active: bool,
    /// `true` if the device has a pending NMI edge (drained via `IoDevice::take_nmi`).
    pub nmi: bool,
    /// `true` if the device has a pending CPU reset request (drained via `IoDevice::take_reset`).
    pub reset: bool,
}

/// The CPU interrupt inputs a device can be wired to, or that a CPU declares it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterruptLine {
    /// The maskable interrupt request every supported CPU has.
    Irq,
    /// The 6809's fast interrupt request: maskable separately from IRQ, and of higher priority.
    Firq,
    /// The non-maskable interrupt.
    Nmi,
}

impl InterruptLine {
    /// The line's name as configuration spells it (`irq`, `firq`, `nmi`).
    pub fn name(self) -> &'static str {
        match self {
            InterruptLine::Irq => "irq",
            InterruptLine::Firq => "firq",
            InterruptLine::Nmi => "nmi",
        }
    }
}

impl std::fmt::Display for InterruptLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Tracks the state of the IRQ and FIRQ lines (level-triggered, multi-source) and NMI
/// (edge-triggered).
pub struct InterruptController {
    /// Every source currently requesting an interrupt, on either maskable line.
    irq_sources: u64,
    /// Subset of `irq_sources` asserted via [`assert_irq_pulse`](InterruptController::assert_irq_pulse);
    /// cleared from `irq_sources` the next time an IRQ is serviced.
    irq_pulse_sources: u64,
    /// Sources wired to FIRQ rather than IRQ. Fixed once the machine is built; always zero
    /// for a CPU without FIRQ.
    firq_mask: u64,
    nmi_pending: bool,
    reset_pending: bool,
}

impl InterruptController {
    /// Creates a new controller with no active interrupts.
    pub fn new() -> Self {
        Self {
            irq_sources: 0,
            irq_pulse_sources: 0,
            firq_mask: 0,
            nmi_pending: false,
            reset_pending: false,
        }
    }

    /// Asserts the IRQ line from `source`. The line remains active until all sources release it.
    pub fn assert_irq(&mut self, source: IrqSource) {
        self.irq_sources |= 1u64 << source.0;
    }

    /// Asserts the IRQ line from `source` in one-shot ("pulse") mode: `source` is
    /// automatically released the next time an IRQ is serviced, rather than requiring
    /// an explicit [`release_irq`](InterruptController::release_irq) call.
    ///
    /// Used to give a level-triggered source "trigger exactly one IRQ" semantics —
    /// e.g. a UI control asserting IRQ while the CPU is free-running, where a sustained
    /// level would otherwise re-service the same interrupt on every instruction after
    /// `RTI` until manually released.
    pub fn assert_irq_pulse(&mut self, source: IrqSource) {
        let bit = 1u64 << source.0;
        self.irq_sources |= bit;
        self.irq_pulse_sources |= bit;
    }

    /// Releases `source` from the IRQ line. If no other sources are asserting, the line goes low.
    /// Also clears any pending one-shot state for `source`.
    pub fn release_irq(&mut self, source: IrqSource) {
        let mask = !(1u64 << source.0);
        self.irq_sources &= mask;
        self.irq_pulse_sources &= mask;
    }

    /// Routes the sources whose bits are set in `mask` to the FIRQ line instead of IRQ.
    /// Called once, when the machine is built; see the [module docs](self#firq-routing).
    pub fn set_firq_mask(&mut self, mask: u64) {
        self.firq_mask = mask;
    }

    /// Returns the sources routed to FIRQ: bit `n` set means `IrqSource(n)` drives FIRQ.
    #[inline]
    pub fn firq_mask(&self) -> u64 {
        self.firq_mask
    }

    /// Returns `true` if any source routed to IRQ is currently asserting the IRQ line.
    #[inline]
    pub fn irq_active(&self) -> bool {
        self.irq_sources & !self.firq_mask != 0
    }

    /// Returns `true` if any source routed to FIRQ is currently asserting the FIRQ line.
    #[inline]
    pub fn firq_active(&self) -> bool {
        self.irq_sources & self.firq_mask != 0
    }

    /// Releases all IRQ-routed sources currently asserted in pulse mode. Called once an IRQ
    /// has been serviced (i.e. the CPU has entered the ISR), so each pulsed source produces
    /// exactly one interrupt regardless of how long the line would otherwise remain
    /// asserted. Pulsed sources routed to FIRQ stay pending.
    #[inline]
    pub fn consume_irq_pulses(&mut self) {
        let consumed = self.irq_pulse_sources & !self.firq_mask;
        self.irq_sources &= !consumed;
        self.irq_pulse_sources &= !consumed;
    }

    /// Latches a pending NMI. Called on the falling edge of the NMI line.
    pub fn signal_nmi(&mut self) {
        self.nmi_pending = true;
    }

    /// Consumes and clears the pending NMI flag. Returns `true` if an NMI was pending.
    #[inline]
    pub fn take_nmi(&mut self) -> bool {
        let pending = self.nmi_pending;
        self.nmi_pending = false;
        pending
    }

    /// Returns `true` if an NMI is pending and has not yet been serviced.
    #[inline]
    pub fn nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    /// Latches a pending RESET. Called on the falling edge of the RESET line.
    pub fn signal_reset(&mut self) {
        self.reset_pending = true;
    }

    /// Consumes and clears the pending RESET flag. Returns `true` if RESET was pending.
    #[inline]
    pub fn take_reset(&mut self) -> bool {
        let pending = self.reset_pending;
        self.reset_pending = false;
        pending
    }

    /// Returns `true` if RESET is pending and has not yet been serviced.
    #[inline]
    pub fn reset_pending(&self) -> bool {
        self.reset_pending
    }

    /// Syncs device IRQ, NMI, and RESET state into the controller. Called by the CPU after
    /// each instruction with each device's [`DeviceInterruptState`]: `irq_active` is synced
    /// into the source bitmask, `nmi` (if `true`) latches a pending NMI via
    /// [`signal_nmi`](Self::signal_nmi), and `reset` (if `true`) latches a pending RESET via
    /// [`signal_reset`](Self::signal_reset).
    pub fn poll_devices(&mut self, states: impl Iterator<Item = DeviceInterruptState>) {
        for state in states {
            if state.id.0 < MAX_IRQ_SOURCES {
                let bit = 1u64 << Self::bit_index(IrqSource::from(state.id));
                if state.irq_active {
                    self.irq_sources |= bit;
                } else {
                    self.irq_sources &= !bit;
                }
            }
            if state.nmi {
                self.signal_nmi();
            }
            if state.reset {
                self.signal_reset();
            }
        }
    }

    /// Returns a bitmask of currently-asserting IRQ sources: bit `n` set means `IrqSource(n)`
    /// is asserting. Sources routed to FIRQ are left out. Priority order matches bit order (lower bit = higher priority, per
    /// `IrqSource`'s `Ord` impl), so a [`VectorResolver`] can mask this against its own
    /// priority/enable mask and find the highest-priority active source directly via
    /// `trailing_zeros()`, with no need to reconstruct a bitmask from an iterator. Unlike
    /// [`take_nmi`](Self::take_nmi) and [`take_reset`](Self::take_reset), this does not mutate
    /// state — a `VectorResolver` only has `&self` access to the controller when resolving a
    /// vector address.
    ///
    /// [`VectorResolver`]: crate::bus::vector::VectorResolver
    #[inline]
    pub fn active_sources_mask(&self) -> u64 {
        self.irq_sources & !self.firq_mask
    }

    /// Validates and extracts the bit index for `source`.
    #[inline]
    fn bit_index(source: IrqSource) -> u32 {
        assert!(
            source.0 < 64,
            "IrqSource({}) is out of range: InterruptController supports IrqSource values 0..64",
            source.0
        );
        source.0
    }
}

impl Default for InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irq_inactive_by_default() {
        assert!(!InterruptController::new().irq_active());
    }

    #[test]
    fn assert_makes_irq_active() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(1));
        assert!(ctrl.irq_active());
    }

    #[test]
    fn release_all_sources_clears_irq() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(1));
        ctrl.assert_irq(IrqSource(2));
        ctrl.release_irq(IrqSource(1));
        assert!(ctrl.irq_active());
        ctrl.release_irq(IrqSource(2));
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn release_nonexistent_source_is_harmless() {
        let mut ctrl = InterruptController::new();
        ctrl.release_irq(IrqSource(63));
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn nmi_inactive_by_default() {
        assert!(!InterruptController::new().nmi_pending());
    }

    #[test]
    fn signal_nmi_latches_pending() {
        let mut ctrl = InterruptController::new();
        ctrl.signal_nmi();
        assert!(ctrl.nmi_pending());
    }

    #[test]
    fn take_nmi_returns_true_and_clears() {
        let mut ctrl = InterruptController::new();
        ctrl.signal_nmi();
        assert!(ctrl.take_nmi());
        assert!(!ctrl.nmi_pending());
    }

    #[test]
    fn take_nmi_returns_false_when_not_pending() {
        let mut ctrl = InterruptController::new();
        assert!(!ctrl.take_nmi());
    }

    #[test]
    fn reset_inactive_by_default() {
        assert!(!InterruptController::new().reset_pending());
    }

    #[test]
    fn signal_reset_latches_pending() {
        let mut ctrl = InterruptController::new();
        ctrl.signal_reset();
        assert!(ctrl.reset_pending());
    }

    #[test]
    fn take_reset_returns_true_and_clears() {
        let mut ctrl = InterruptController::new();
        ctrl.signal_reset();
        assert!(ctrl.take_reset());
        assert!(!ctrl.reset_pending());
    }

    #[test]
    fn take_reset_returns_false_when_not_pending() {
        let mut ctrl = InterruptController::new();
        assert!(!ctrl.take_reset());
    }

    /// Builds a `DeviceInterruptState` tersely for test iterators.
    fn dis(id: u32, irq_active: bool, nmi: bool, reset: bool) -> DeviceInterruptState {
        DeviceInterruptState {
            id: DeviceId(id),
            irq_active,
            nmi,
            reset,
        }
    }

    #[test]
    fn poll_devices_asserts_active_sources() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(1, true, false, false), dis(2, false, false, false)].into_iter());
        assert!(ctrl.irq_active());
    }

    #[test]
    fn poll_devices_releases_inactive_sources() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(1));
        ctrl.poll_devices([dis(1, false, false, false)].into_iter());
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn poll_devices_syncs_multiple_sources() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(1, true, false, false), dis(2, true, false, false)].into_iter());
        assert!(ctrl.irq_active());
        ctrl.poll_devices([dis(1, false, false, false), dis(2, false, false, false)].into_iter());
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn poll_devices_latches_nmi_from_device_state() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(1, false, true, false)].into_iter());
        assert!(ctrl.nmi_pending());
    }

    #[test]
    fn poll_devices_does_not_latch_nmi_when_not_reported() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(1, true, false, false)].into_iter());
        assert!(!ctrl.nmi_pending());
    }

    #[test]
    fn poll_devices_latches_reset_from_device_state() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(1, false, false, true)].into_iter());
        assert!(ctrl.reset_pending());
    }

    #[test]
    fn poll_devices_does_not_latch_reset_when_not_reported() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(1, true, false, false)].into_iter());
        assert!(!ctrl.reset_pending());
    }

    #[test]
    fn assert_irq_pulse_makes_irq_active() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq_pulse(IrqSource(1));
        assert!(ctrl.irq_active());
    }

    #[test]
    fn consume_irq_pulses_releases_pulsed_source() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq_pulse(IrqSource(1));
        ctrl.consume_irq_pulses();
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn consume_irq_pulses_does_not_release_manually_asserted_source() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(1));
        ctrl.consume_irq_pulses();
        assert!(ctrl.irq_active());
    }

    #[test]
    fn consume_irq_pulses_leaves_other_manual_sources_asserted() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(1));
        ctrl.assert_irq_pulse(IrqSource(2));
        ctrl.consume_irq_pulses();
        assert!(ctrl.irq_active());
        ctrl.release_irq(IrqSource(1));
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn release_irq_clears_pulse_state() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq_pulse(IrqSource(1));
        ctrl.release_irq(IrqSource(1));
        // If pulse state were left dangling, a later manual assert of a
        // different source would be wrongly consumed as a pulse.
        ctrl.assert_irq(IrqSource(1));
        ctrl.consume_irq_pulses();
        assert!(ctrl.irq_active());
    }

    #[test]
    fn irq_source_from_device_id() {
        let src = IrqSource::from(DeviceId(42));
        assert_eq!(src, IrqSource(42));
    }

    #[test]
    fn poll_devices_does_not_panic_on_non_irq_source() {
        let mut ctrl = InterruptController::new();
        ctrl.poll_devices([dis(MAX_IRQ_SOURCES, true, false, false)].into_iter());
        assert!(!ctrl.irq_active());
    }

    #[test]
    fn firq_inactive_by_default() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(1));
        assert!(!ctrl.firq_active());
    }

    #[test]
    fn firq_routed_source_raises_firq_not_irq() {
        let mut ctrl = InterruptController::new();
        ctrl.set_firq_mask(1 << 2);
        ctrl.poll_devices([dis(2, true, false, false)].into_iter());
        assert!(ctrl.firq_active());
        assert!(!ctrl.irq_active());
        assert_eq!(ctrl.active_sources_mask(), 0);
    }

    #[test]
    fn irq_and_firq_sources_are_independent() {
        let mut ctrl = InterruptController::new();
        ctrl.set_firq_mask(1 << 2);
        ctrl.poll_devices([dis(1, true, false, false), dis(2, true, false, false)].into_iter());
        assert!(ctrl.irq_active());
        assert!(ctrl.firq_active());
        assert_eq!(ctrl.active_sources_mask(), 1 << 1);

        ctrl.poll_devices([dis(1, false, false, false)].into_iter());
        assert!(!ctrl.irq_active());
        assert!(ctrl.firq_active());

        ctrl.poll_devices([dis(2, false, false, false)].into_iter());
        assert!(!ctrl.firq_active());
    }

    #[test]
    fn consume_irq_pulses_leaves_firq_routed_pulses_pending() {
        let mut ctrl = InterruptController::new();
        ctrl.set_firq_mask(1 << 2);
        ctrl.assert_irq_pulse(IrqSource(1));
        ctrl.assert_irq_pulse(IrqSource(2));
        ctrl.consume_irq_pulses();
        assert!(!ctrl.irq_active());
        assert!(ctrl.firq_active());
    }

    #[test]
    fn active_sources_mask_is_zero_when_none_active() {
        let ctrl = InterruptController::new();
        assert_eq!(ctrl.active_sources_mask(), 0);
    }

    #[test]
    fn active_sources_mask_has_a_bit_set_per_active_source() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(5));
        ctrl.assert_irq(IrqSource(1));
        ctrl.assert_irq(IrqSource(3));
        assert_eq!(ctrl.active_sources_mask(), (1 << 1) | (1 << 3) | (1 << 5));
    }

    #[test]
    fn active_sources_mask_lowest_bit_is_highest_priority_source() {
        let mut ctrl = InterruptController::new();
        ctrl.assert_irq(IrqSource(5));
        ctrl.assert_irq(IrqSource(1));
        ctrl.assert_irq(IrqSource(3));
        assert_eq!(ctrl.active_sources_mask().trailing_zeros(), 1);
    }

    #[test]
    fn irq_source_orders_by_ascending_value() {
        assert!(IrqSource(0) < IrqSource(1));
        assert!(IrqSource(63) > IrqSource(0));
    }
}
