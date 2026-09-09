//! A RAM device: a block of directly addressable read/write memory.

use crate::emulator::device::IoDevice;
use rand::RngExt;

/// A block of read/write memory mapped at a fixed base address.
pub struct Ram {
    address: u16,
    data: Vec<u8>,
}

impl Ram {
    /// Creates a new `Ram` of `size` bytes at `address`, with random initial contents.
    pub fn new(address: u16, size: usize) -> Self {
        let mut data = vec![0u8; size];
        rand::rng().fill(&mut data[..]);
        Self { address, data }
    }

    /// Creates a new `Ram` of `size` bytes at `address`, with every cell filled with
    /// `fill_value`.
    pub fn with_fill(address: u16, size: usize, fill_value: u8) -> Self {
        Self {
            address,
            data: vec![fill_value; size],
        }
    }

    /// Creates a new `Ram` at `address`, pre-loaded with `data`.
    pub fn with_data(address: u16, data: Vec<u8>) -> Self {
        Self { address, data }
    }

    fn offset(&self, address: u16) -> usize {
        (address - self.address) as usize
    }
}

impl IoDevice for Ram {
    fn read(&mut self, address: u16) -> u8 {
        self.peek(address)
    }

    fn write(&mut self, address: u16, value: u8) {
        let offset = self.offset(address);
        self.data[offset] = value;
    }

    fn peek(&self, address: u16) -> u8 {
        self.data[self.offset(address)]
    }

    fn name(&self) -> &str {
        "ram"
    }

    fn identity_address(&self) -> u16 {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_round_trip() {
        let mut ram = Ram::with_fill(0x0000, 0x2000, 0);
        ram.write(0x0100, 0xAB);
        assert_eq!(ram.read(0x0100), 0xAB);
    }

    #[test]
    fn patch_read_round_trip() {
        let mut ram = Ram::with_fill(0x0000, 0x2000, 0);
        ram.patch(0x0100, 0xAB);
        assert_eq!(ram.read(0x0100), 0xAB);
    }

    #[test]
    fn with_fill_initializes_every_cell() {
        let ram = Ram::with_fill(0xC000, 256, 0x42);
        for offset in 0..256u16 {
            assert_eq!(ram.peek(0xC000 + offset), 0x42);
        }
    }

    #[test]
    fn with_data_preloads_initial_contents() {
        let data = vec![0xABu8; 256];
        let ram = Ram::with_data(0xC000, data);
        assert_eq!(ram.peek(0xC042), 0xAB);
    }

    #[test]
    fn with_data_allows_writes() {
        let data = vec![0xABu8; 256];
        let mut ram = Ram::with_data(0xC000, data);
        ram.write(0xC042, 0x99);
        assert_eq!(ram.read(0xC042), 0x99);
    }

    #[test]
    fn offset_translation() {
        let mut ram = Ram::with_fill(0xDF00, 16, 0);
        ram.write(0xDF05, 0x42);
        assert_eq!(ram.peek(0xDF05), 0x42);
        assert_eq!(ram.peek(0xDF00), 0x00);
    }

    #[test]
    fn identity_address_is_base_address() {
        let ram = Ram::with_fill(0xC000, 256, 0);
        assert_eq!(ram.identity_address(), 0xC000);
    }
}
