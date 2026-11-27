//! A fixed-column output line, for the per-family row formats.

/// A single output line under construction, addressed by absolute byte column.
pub struct Row(Vec<u8>);

impl Row {
    pub fn new() -> Self {
        Row(Vec::new())
    }

    /// Writes `text` starting at `col`, padding with spaces as needed to reach it.
    /// Never shortens the row: writing an empty string is a no-op past the pad.
    pub fn put(&mut self, col: usize, text: &str) {
        while self.0.len() < col {
            self.0.push(b' ');
        }
        for (i, b) in text.bytes().enumerate() {
            let idx = col + i;
            if idx < self.0.len() {
                self.0[idx] = b;
            } else {
                self.0.push(b);
            }
        }
    }

    pub fn into_string(self) -> String {
        String::from_utf8(self.0).expect("row contents are always ASCII")
    }
}

/// Formats instruction bytes as space-separated hex pairs.
pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Formats a label line: the label at `col`, followed by `:`.
pub fn label_line(col: usize, label: &str) -> String {
    let mut row = Row::new();
    row.put(col, label);
    let mut s = row.into_string();
    s.push(':');
    s
}

/// Renders a flags field: each name character whose bit (from the most
/// significant, for an 8-character `names`) is set in `bits`, else a space.
pub fn flags_field(names: &str, bits: u8) -> String {
    names
        .chars()
        .enumerate()
        .map(|(i, c)| if bits & (0x80 >> i) != 0 { c } else { ' ' })
        .collect()
}
