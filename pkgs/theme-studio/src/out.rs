//! The terminal output. Each frame is collected and, before it goes out,
//! the double-underline marker (rapid blink, which the studio never uses
//! otherwise) becomes a double underline: ratatui itself only knows single
//! underlines.

use std::io::{Stdout, Write, stdout};

pub struct Out {
    buf: Vec<u8>,
    out: Stdout,
}

impl Default for Out {
    fn default() -> Self {
        Out {
            buf: Vec::with_capacity(1 << 16),
            out: stdout(),
        }
    }
}

/// Replaces every `from` in `buf` with `to`.
fn replace(buf: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        if buf[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(buf[i]);
            i += 1;
        }
    }
    out
}

/// The frame with the marker (on: SGR 6, off: SGR 25) turned into a double
/// underline (SGR 4:2, off: SGR 24).
pub fn rewrite(buf: &[u8]) -> Vec<u8> {
    let b = replace(buf, b"\x1b[6m", b"\x1b[4:2m");
    replace(&b, b"\x1b[25m", b"\x1b[24m")
}

impl Write for Out {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            let frame = if self
                .buf
                .windows(4)
                .any(|w| w == b"\x1b[6m" || w == b"\x1b[25")
            {
                rewrite(&self.buf)
            } else {
                std::mem::take(&mut self.buf)
            };
            self.buf.clear();
            self.out.write_all(&frame)?;
        }
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_becomes_a_double_underline() {
        assert_eq!(rewrite(b"a\x1b[6mbc\x1b[25md"), b"a\x1b[4:2mbc\x1b[24md");
        assert_eq!(rewrite(b"\x1b[4mx\x1b[24m"), b"\x1b[4mx\x1b[24m");
    }
}
