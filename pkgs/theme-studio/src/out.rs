//! The terminal output. Each frame is collected and, before it goes out,
//! the hover marker (rapid blink, which the studio never uses otherwise)
//! becomes the chosen underline style: ratatui itself only knows plain
//! underlines.

use std::io::{Stdout, Write, stdout};

use crate::hit::Underline;

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

/// The frame with the marker turned into `style`'s underline.
pub fn rewrite(buf: &[u8], style: Option<Underline>) -> Vec<u8> {
    let on = match style {
        Some(u) if u != Underline::Single => format!("\x1b[{}m", u.sgr()),
        // nothing to swap: drop the marker
        _ => String::new(),
    };
    let b = replace(buf, b"\x1b[6m", on.as_bytes());
    replace(
        &b,
        b"\x1b[25m",
        if on.is_empty() { b"" } else { b"\x1b[24m" },
    )
}

impl Write for Out {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            let style = crate::hit::look().underline.map(|(u, _)| u);
            let frame = if self
                .buf
                .windows(4)
                .any(|w| w == b"\x1b[6m" || w == b"\x1b[25")
            {
                rewrite(&self.buf, style)
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
    fn marker_becomes_the_underline_style() {
        let frame = b"a\x1b[6mbc\x1b[25md";
        assert_eq!(
            rewrite(frame, Some(Underline::Curly)),
            b"a\x1b[4:3mbc\x1b[24md"
        );
        assert_eq!(
            rewrite(frame, Some(Underline::Dashed)),
            b"a\x1b[4:5mbc\x1b[24md"
        );
        assert_eq!(rewrite(frame, None), b"abcd");
        assert_eq!(
            rewrite(b"\x1b[4mx\x1b[24m", Some(Underline::Curly)),
            b"\x1b[4mx\x1b[24m"
        );
    }
}
