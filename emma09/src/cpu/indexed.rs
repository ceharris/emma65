//! The indexed-addressing postbyte, as far as it can be decoded without CPU
//! state. The CPU computes the effective address itself (it updates the
//! index register for the auto-increment and auto-decrement forms).
//!
//! | Postbyte | Form | Extra cycles (indirect) | Extra bytes |
//! |---|---|---|---|
//! | `0RRnnnnn` | `n5,R` | 1 (—) | 0 |
//! | `1RR00000` | `,R+` | 2 (—) | 0 |
//! | `1RRi0001` | `,R++` | 3 (6) | 0 |
//! | `1RR00010` | `,-R` | 2 (—) | 0 |
//! | `1RRi0011` | `,--R` | 3 (6) | 0 |
//! | `1RRi0100` | `,R` | 0 (3) | 0 |
//! | `1RRi0101` | `B,R` | 1 (4) | 0 |
//! | `1RRi0110` | `A,R` | 1 (4) | 0 |
//! | `1RRi1000` | `n8,R` | 1 (4) | 1 |
//! | `1RRi1001` | `n16,R` | 4 (7) | 2 |
//! | `1RRi1011` | `D,R` | 4 (7) | 0 |
//! | `1RRi1100` | `n8,PCR` | 1 (4) | 1 |
//! | `1RRi1101` | `n16,PCR` | 5 (8) | 2 |
//! | `1RR11111` | `[n16]` | (5) | 2 |
//!
//! `RR` selects X, Y, U or S, and `i` makes the form indirect. The other
//! postbytes are undefined: `1RRx0111`, `1RRx1010`, `1RRx1110`, `1RR01111`,
//! and the indirect forms of `,R+` and `,-R`. The CPU treats an instruction
//! with an undefined postbyte as an invalid opcode.

/// Bit 7 clear: a 5-bit signed offset from the index register.
pub const OFFSET5: u8 = 0x80;
/// Bit 4 of a non-5-bit postbyte: the form is indirect.
pub const INDIRECT: u8 = 0x10;

/// Returns the number of offset or address bytes that follow `postbyte`.
pub fn extra_bytes(postbyte: u8) -> u16 {
    if postbyte & OFFSET5 == 0 {
        return 0;
    }
    match postbyte & 0x0F {
        0x8 | 0xC => 1,
        0x9 | 0xD | 0xF => 2,
        _ => 0,
    }
}

/// Returns `true` if `postbyte` is one of the documented forms.
pub fn is_defined(postbyte: u8) -> bool {
    if postbyte & OFFSET5 == 0 {
        return true;
    }
    let indirect = postbyte & INDIRECT != 0;
    match postbyte & 0x0F {
        0x0 | 0x2 => !indirect,
        0xF => indirect,
        0x7 | 0xA | 0xE => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_bytes_by_form() {
        assert_eq!(extra_bytes(0x1F), 0); // -1,X
        assert_eq!(extra_bytes(0x84), 0); // ,X
        assert_eq!(extra_bytes(0x88), 1); // n8,X
        assert_eq!(extra_bytes(0xA9), 2); // n16,Y
        assert_eq!(extra_bytes(0x8C), 1); // n8,PCR
        assert_eq!(extra_bytes(0x9D), 2); // [n16,PCR]
        assert_eq!(extra_bytes(0x9F), 2); // [n16]
        assert_eq!(extra_bytes(0x8B), 0); // D,X
    }

    #[test]
    fn defined_postbytes() {
        // 32 five-bit offsets × 4 registers, plus per register 12
        // non-indirect forms and 11 indirect ones (counting [n16], which
        // ignores RR).
        let count = (0..=255u8).filter(|&p| is_defined(p)).count();
        assert_eq!(count, 128 + 4 * (12 + 11));
        for undefined in [0x87, 0x8A, 0x8E, 0x8F, 0x90, 0x92, 0x97, 0x9A, 0x9E] {
            assert!(!is_defined(undefined), "${undefined:02X}");
        }
        assert!(is_defined(0x9F));
        assert!(is_defined(0xBF)); // [n16] ignores RR
    }
}
