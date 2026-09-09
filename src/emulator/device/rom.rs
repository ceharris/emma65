//! A ROM device: read-only memory with a configurable write policy.

use crate::emulator::bus::RomWritePolicy;
use crate::emulator::device::IoDevice;
use crate::emulator::error::BusError;

/// A block of read-only memory mapped at a fixed base address.
pub struct Rom {
    address: u16,
    data: Vec<u8>,
    write_policy: RomWritePolicy,
}

impl Rom {
    /// Creates a new `Rom` at `address`, pre-loaded with `data`, enforcing `write_policy`
    /// for writes.
    pub fn new(address: u16, data: Vec<u8>, write_policy: RomWritePolicy) -> Self {
        Self {
            address,
            data,
            write_policy,
        }
    }

    fn offset(&self, address: u16) -> usize {
        (address - self.address) as usize
    }
}

impl IoDevice for Rom {
    fn read(&mut self, address: u16) -> u8 {
        self.peek(address)
    }

    fn check_writability(&self, address: u16) -> Result<(), BusError> {
        match self.write_policy {
            RomWritePolicy::Ignore => Ok(()),
            RomWritePolicy::Error => Err(BusError::RomWrite { addr: address }),
        }
    }

    fn write(&mut self, _address: u16, _value: u8) {
        // ROM contents never change via a normal write, regardless of `write_policy` --
        // the policy only governs whether the bus reports the write as accepted or
        // rejected (see `check_writability`). Use `patch()` to modify ROM contents.
    }

    fn peek(&self, address: u16) -> u8 {
        self.data[self.offset(address)]
    }

    fn patch(&mut self, address: u16, value: u8) {
        let offset = self.offset(address);
        self.data[offset] = value;
    }

    fn name(&self) -> &str {
        "rom"
    }

    fn identity_address(&self) -> u16 {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_ignore_policy() {
        let data = vec![0xEAu8; 256];
        let mut rom = Rom::new(0xC000, data, RomWritePolicy::Ignore);
        assert!(rom.check_writability(0xC010).is_ok());
        rom.write(0xC010, 0x00);
        assert_eq!(rom.read(0xC010), 0xEA);
    }

    #[test]
    fn read_only_error_policy() {
        let data = vec![0xEAu8; 256];
        let rom = Rom::new(0xC000, data, RomWritePolicy::Error);
        assert!(matches!(
            rom.check_writability(0xC010),
            Err(BusError::RomWrite { addr: 0xC010 })
        ));
    }

    #[test]
    fn write_never_mutates_data_regardless_of_policy() {
        let data = vec![0xEAu8; 256];
        let mut rom = Rom::new(0xC000, data, RomWritePolicy::Ignore);
        rom.write(0xC010, 0x00);
        assert_eq!(rom.peek(0xC010), 0xEA);
    }

    #[test]
    fn patch_read_round_trip() {
        let data = vec![0xEAu8; 256];
        let mut rom = Rom::new(0xC000, data, RomWritePolicy::Error);
        rom.patch(0xC000, 0xAB);
        assert_eq!(rom.read(0xC000), 0xAB);
    }

    #[test]
    fn offset_translation() {
        let mut data = vec![0xEAu8; 256];
        data[5] = 0x42;
        let rom = Rom::new(0xC000, data, RomWritePolicy::Ignore);
        assert_eq!(rom.peek(0xC005), 0x42);
    }

    #[test]
    fn identity_address_is_base_address() {
        let data = vec![0xEAu8; 256];
        let rom = Rom::new(0xC000, data, RomWritePolicy::Ignore);
        assert_eq!(rom.identity_address(), 0xC000);
    }
}
