//! Source text with Roslyn-style line and column positions.
//!
//! Columns count Unicode scalar values, which match Roslyn's UTF-16 columns for
//! the Basic Multilingual Plane. The UTF-8 BOM is not part of the text.

use std::path::Path;

#[derive(Clone, Debug)]
pub struct Source {
    pub text: String,
    line_starts: Vec<usize>,
}

impl Source {
    pub fn new(text: String) -> Self {
        let mut line_starts = vec![0];
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' {
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                line_starts.push(i);
            } else if bytes[i] == b'\n' {
                i += 1;
                line_starts.push(i);
            } else {
                i += 1;
            }
        }
        Self { text, line_starts }
    }

    pub fn read(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let bytes = strip_bom(&bytes);
        let text = String::from_utf8_lossy(bytes).into_owned();
        Ok(Self::new(text))
    }

    pub fn line_column(&self, byte: usize) -> (u32, u32) {
        let byte = byte.min(self.text.len());
        let line_index = match self.line_starts.binary_search(&byte) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        let start = self.line_starts[line_index];
        let column = self.text[start..byte].chars().count();
        (line_index as u32 + 1, column as u32 + 1)
    }

    pub fn line_index(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        match self.line_starts.binary_search(&byte) {
            Ok(index) => index,
            Err(index) => index - 1,
        }
    }

    /// Content bounds of line `index`, excluding the newline.
    pub fn line_content(&self, index: usize) -> (usize, usize) {
        let start = self.line_starts[index];
        let end = if index + 1 < self.line_starts.len() {
            let mut next = self.line_starts[index + 1];
            if next > start && self.text.as_bytes()[next - 1] == b'\n' {
                next -= 1;
                if next > start && self.text.as_bytes()[next - 1] == b'\r' {
                    next -= 1;
                }
            }
            next
        } else {
            self.text.len()
        };
        (start, end)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn slice(&self, start: u32, end: u32) -> &str {
        let start = start as usize;
        let end = (end as usize).min(self.text.len());
        if start > end || start > self.text.len() {
            return "";
        }
        &self.text[start..end]
    }
}

pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    }
}

pub fn is_cs_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\u{000B}' | '\u{000C}')
}

pub fn is_cs_newline(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}')
}

/// .NET `char.IsWhiteSpace`, close enough for ASCII and the usual Unicode spaces.
pub fn is_dotnet_whitespace(ch: char) -> bool {
    ch.is_whitespace()
}
