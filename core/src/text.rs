#[derive(Debug, Clone)]
pub struct Text<'a> {
    buf: &'a str,
    start: usize,
    current: usize,
}

impl<'a> Text<'a> {
    pub fn from(buf: &'a str) -> Self {
        Self {
            buf,
            start: 0,
            current: 0,
        }
    }

    pub fn is_at_end(&self) -> bool {
        self.current == self.buf.len()
    }

    pub fn advance(&mut self) -> Option<u8> {
        if self.is_at_end() {
            return None;
        }
        let c = self.buf.as_bytes()[self.current];

        self.current += 1;

        Some(c)
    }

    pub fn peek(&self) -> Option<u8> {
        if self.is_at_end() {
            return None;
        }
        Some(self.buf.as_bytes()[self.current])
    }

    pub fn skip(&mut self) {
        self.current += 1;
        self.start += 1;
    }

    pub fn consume(&mut self) -> &'a str {
        let text = &self.buf[self.start..self.current];
        self.start = self.current;
        text
    }
}

/// Control-code names for bytes `0x00`–`0x1F`, three characters each.
const ASCII_CTRL_MNEMONICS: &str = "NULSOHSTXETXEOTENQACKBELBS HT LF VT FF CR SO SI \
                                    DLEDC1DC2DC3DC4NAKSYNETBCANEM SUBESCFS GS RS US ";

/// Formats a byte as the alternate representations a disassembler shows
/// beside an 8-bit immediate operand: the decimal value, plus an ASCII
/// rendering (control-code name, quoted printable character, `SPC`, or `DEL`)
/// or, for a non-ASCII byte, its signed value. Starts with `"; "`.
pub fn immediate_comment(operand: u8) -> String {
    if operand < 0x20 {
        let i = 3 * operand as usize;
        format!(
            "; {} ^{} ({})",
            operand,
            (operand + b'@') as char,
            ASCII_CTRL_MNEMONICS[i..i + 3].trim()
        )
    } else if operand == 0x20 {
        "; 32 ' ' (SPC)".to_string()
    } else if operand < 0x7F {
        format!("; {} '{}'", operand, operand as char)
    } else if operand == 0x7F {
        "; 127 (DEL)".to_string()
    } else {
        format!("; {} ({})", operand, operand as i8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_when_not_empty() {
        let t = Text::from("not empty");
        assert!(!t.is_at_end());
    }

    #[test]
    fn test_from_when_empty() {
        let t = Text::from("");
        assert!(t.is_at_end());
    }

    #[test]
    fn test_advance_and_consume() {
        let mut t = Text::from("abc");
        assert!(!t.is_at_end());
        assert_eq!(t.advance(), Some(b'a'));
        assert_eq!(t.advance(), Some(b'b'));
        assert!(!t.is_at_end());
        assert_eq!(t.consume(), "ab");
        assert_eq!(t.advance(), Some(b'c'));
        assert!(t.is_at_end());
        assert_eq!(t.consume(), "c");
    }

    #[test]
    fn test_peek() {
        let mut t = Text::from("a");
        assert!(!t.is_at_end());
        assert_eq!(t.peek(), Some(b'a'));
        assert!(!t.is_at_end());
        assert_eq!(t.advance(), Some(b'a'));
        assert!(t.is_at_end());
    }
}
