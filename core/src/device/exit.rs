//! The `exit` device: lets a program end the emulator run, with an exit
//! status, on any CPU.

use super::IoDevice;
use crate::bus::CpuRequest;
use crate::{LogCategory, LogLevel, LogSender, log_msg};

/// The bytes a program writes, in order, before the status byte.
pub const UNLOCK: [u8; 4] = *b"EXIT";

/// A one-byte, write-only register that stops the CPU for good when a
/// program writes the ASCII bytes `E`, `X`, `I`, `T` and then a status byte.
///
/// The run then ends as it does after the 65C02's STP, and `emma6k` exits
/// with the status byte as its process exit code. Any byte that breaks the
/// sequence starts it over (an `E` starts a new one), so a stray write or a
/// loop clearing memory can't end the run by accident. Reads return 0. A
/// reset forgets a partly written sequence and logs that the device was
/// reset, as the other devices do, so the log shows it's configured.
#[derive(Default)]
pub struct ExitDevice {
    /// The register's bus address.
    address: u16,
    /// How many bytes of [`UNLOCK`] have been written so far.
    matched: usize,
    /// The exit request, once the status byte has been written.
    request: Option<u8>,
    /// Where the reset message goes.
    log_sender: LogSender,
}

impl ExitDevice {
    /// Creates the device for a register at `address`, with no sequence in
    /// progress.
    pub fn new(address: u16) -> Self {
        Self {
            address,
            ..Self::default()
        }
    }

    /// Sets where the device sends its log messages.
    pub fn set_log_sender(&mut self, sender: LogSender) {
        self.log_sender = sender;
    }
}

impl IoDevice for ExitDevice {
    fn read(&mut self, _address: u16) -> u8 {
        0
    }

    fn write(&mut self, _address: u16, value: u8) {
        if self.matched == UNLOCK.len() {
            self.request = Some(value);
            self.matched = 0;
        } else if value == UNLOCK[self.matched] {
            self.matched += 1;
        } else {
            self.matched = usize::from(value == UNLOCK[0]);
        }
    }

    fn peek(&self, _address: u16) -> u8 {
        0
    }

    fn reset(&mut self) {
        self.matched = 0;
        log_msg!(
            self.log_sender,
            LogLevel::Info,
            LogCategory::Device,
            "{} reset",
            self.identity()
        );
    }

    fn take_request(&mut self) -> Option<CpuRequest> {
        self.request.take().map(CpuRequest::Exit)
    }

    /// It only acts on writes, so the bus doesn't poll it every step.
    fn is_passive(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "exit"
    }

    fn identity_address(&self) -> u16 {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_all(device: &mut ExitDevice, bytes: &[u8]) {
        for &b in bytes {
            device.write(0, b);
        }
    }

    #[test]
    fn the_sequence_then_a_status_requests_an_exit() {
        let mut device = ExitDevice::new(0xFFD2);
        write_all(&mut device, b"EXIT");
        assert_eq!(device.take_request(), None);
        device.write(0, 42);
        assert_eq!(device.take_request(), Some(CpuRequest::Exit(42)));
        assert_eq!(device.take_request(), None);
    }

    #[test]
    fn a_wrong_byte_starts_the_sequence_over() {
        let mut device = ExitDevice::new(0xFFD2);
        write_all(&mut device, b"EXAT\x00");
        assert_eq!(device.take_request(), None);
        write_all(&mut device, b"EXIEXIT\x05");
        assert_eq!(device.take_request(), Some(CpuRequest::Exit(5)));
    }

    #[test]
    fn an_e_mid_sequence_starts_a_new_one() {
        let mut device = ExitDevice::new(0xFFD2);
        write_all(&mut device, b"EEXIT\x01");
        assert_eq!(device.take_request(), Some(CpuRequest::Exit(1)));
    }

    #[test]
    fn zeros_and_stray_bytes_never_exit() {
        let mut device = ExitDevice::new(0xFFD2);
        for b in 0..=255u8 {
            device.write(0, b);
            device.write(0, 0);
        }
        assert_eq!(device.take_request(), None);
    }

    #[test]
    fn the_sequence_can_be_repeated() {
        let mut device = ExitDevice::new(0xFFD2);
        write_all(&mut device, b"EXIT\x00EXIT\x02");
        assert_eq!(device.take_request(), Some(CpuRequest::Exit(2)));
    }

    #[test]
    fn reset_forgets_a_partial_sequence() {
        let mut device = ExitDevice::new(0xFFD2);
        write_all(&mut device, b"EXI");
        device.reset();
        write_all(&mut device, b"T\x00");
        assert_eq!(device.take_request(), None);
    }

    #[test]
    fn reset_logs_device_message() {
        let (sender, rx) = crate::logging::log_channel(4);
        let mut device = ExitDevice::new(0xFFD2);
        device.set_log_sender(sender);
        device.reset();
        let received = rx.recv().unwrap();
        assert_eq!(received.level, LogLevel::Info);
        assert_eq!(received.category, LogCategory::Device);
        assert_eq!(received.message, "exit@0xffd2 reset");
    }

    #[test]
    fn reads_return_zero() {
        let mut device = ExitDevice::new(0xFFD2);
        write_all(&mut device, b"EXIT");
        assert_eq!(device.read(0), 0);
        assert_eq!(device.peek(0), 0);
    }
}
