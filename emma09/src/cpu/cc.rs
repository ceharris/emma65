//! The condition-code register (CC) as a bitflags newtype over `u8`.

use bitflags::bitflags;

bitflags! {
    /// The 6809's condition-code register.
    ///
    /// From bit 7 down: E (entire state stacked), F (FIRQ mask), H (half
    /// carry), I (IRQ mask), N (negative), Z (zero), V (overflow), C (carry).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConditionCodes: u8 {
        const C = 0x01;
        const V = 0x02;
        const Z = 0x04;
        const N = 0x08;
        const I = 0x10;
        const H = 0x20;
        const F = 0x40;
        const E = 0x80;
    }
}

impl ConditionCodes {
    /// Creates condition codes from a raw byte. All eight bits are defined.
    pub fn from_byte(byte: u8) -> Self {
        Self::from_bits_retain(byte)
    }

    /// Returns the condition codes as a raw byte.
    pub fn to_byte(self) -> u8 {
        self.bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_byte_round_trip() {
        for b in 0u8..=255 {
            assert_eq!(ConditionCodes::from_byte(b).to_byte(), b);
        }
    }

    #[test]
    fn flag_bits_match_the_datasheet() {
        assert_eq!(
            (ConditionCodes::E | ConditionCodes::I | ConditionCodes::C).to_byte(),
            0x91
        );
        assert_eq!(ConditionCodes::F.bits(), 0x40);
        assert_eq!(ConditionCodes::H.bits(), 0x20);
        assert_eq!(ConditionCodes::N.bits(), 0x08);
        assert_eq!(ConditionCodes::Z.bits(), 0x04);
        assert_eq!(ConditionCodes::V.bits(), 0x02);
    }
}
