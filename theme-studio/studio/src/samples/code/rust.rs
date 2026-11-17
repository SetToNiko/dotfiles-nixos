use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;

/// A palette: colour names to hex values, in the order they were read.
#[derive(Debug, Default)]
pub struct Palette {
    name: String,
    colours: Vec<(String, u32)>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    BadLine { line: usize, text: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "reading: {e}"),
            LoadError::BadLine { line, text } => write!(f, "line {line}: {text:?}"),
        }
    }
}

impl Palette {
    /// Reads `name = #rrggbb` lines; blank lines and `;` comments are skipped.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let text = fs::read_to_string(path).map_err(LoadError::Io)?;
        let mut colours = Vec::with_capacity(34);
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            let Some((key, hex)) = line.split_once('=') else {
                return Err(LoadError::BadLine { line: i + 1, text: line.into() });
            };
            let hex = hex.trim().trim_start_matches('#');
            match u32::from_str_radix(hex, 16) {
                Ok(v) if hex.len() == 6 => colours.push((key.trim().to_string(), v)),
                _ => return Err(LoadError::BadLine { line: i + 1, text: line.into() }),
            }
        }
        let name = path.file_stem().map_or("untitled".into(), |s| s.to_string_lossy().into());
        Ok(Palette { name, colours })
    }

    /// How many colours share each hue (0..=359), for the overview.
    pub fn hue_counts(&self) -> HashMap<u16, usize> {
        let mut counts = HashMap::new();
        for (_, rgb) in &self.colours {
            let (r, g, b) = ((rgb >> 16) as f32, ((rgb >> 8) & 0xff) as f32, (rgb & 0xff) as f32);
            let hue = (g - b).atan2(r - 0.5 * (g + b)).to_degrees().rem_euclid(360.0);
            *counts.entry(hue as u16).or_insert(0) += 1;
        }
        counts
    }
}
