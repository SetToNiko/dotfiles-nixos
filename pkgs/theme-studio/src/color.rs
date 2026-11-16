//! Colour type and conversions (hex, HSV, HSL, WCAG contrast).

use ratatui::style::Color;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#rrggbb` or `rrggbb` (any case).
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim();
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// Lowercase `#rrggbb`, as in palettes.json.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn color(self) -> Color {
        Color::Rgb(self.0, self.1, self.2)
    }

    /// WCAG relative luminance.
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }

    /// Black or white, whichever reads better on top of this colour.
    pub fn text_on(self) -> Rgb {
        if self.luminance() > 0.18 { Rgb(0, 0, 0) } else { Rgb(255, 255, 255) }
    }
}

/// WCAG contrast ratio between two colours (1.0 to 21.0).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn to_u8(x: f64) -> u8 {
    (x * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Hue in degrees (0..360), saturation and value in 0..1.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Rgb(to_u8(r + m), to_u8(g + m), to_u8(b + m))
}

/// Returns (hue, saturation, value); hue is None for greys.
pub fn rgb_to_hsv(c: Rgb) -> (Option<f64>, f64, f64) {
    let (r, g, b) = (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max };
    (hue(r, g, b, max, d), s, max)
}

/// Hue in degrees, saturation and lightness in 0..1.
pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Rgb {
    let v = l + s * l.min(1.0 - l);
    let sv = if v == 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) };
    hsv_to_rgb(h, sv, v)
}

/// Returns (hue, saturation, lightness); hue is None for greys.
pub fn rgb_to_hsl(c: Rgb) -> (Option<f64>, f64, f64) {
    let (r, g, b) = (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    (hue(r, g, b, max, d), s, l)
}

fn hue(r: f64, g: f64, b: f64, max: f64, d: f64) -> Option<f64> {
    if d == 0.0 {
        return None;
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    Some(h * 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Rgb::parse("#9F6434").unwrap().hex(), "#9f6434");
        assert_eq!(Rgb::parse("2f7d7c"), Some(Rgb(0x2f, 0x7d, 0x7c)));
        assert_eq!(Rgb::parse("#12345"), None);
        assert_eq!(Rgb::parse("#12345g"), None);
    }

    #[test]
    fn hsv_and_hsl_round_trip_every_sampled_colour() {
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(51) {
                    let c = Rgb(r, g, b);
                    let (h, s, v) = rgb_to_hsv(c);
                    assert_eq!(hsv_to_rgb(h.unwrap_or(0.0), s, v), c);
                    let (h, s, l) = rgb_to_hsl(c);
                    assert_eq!(hsl_to_rgb(h.unwrap_or(0.0), s, l), c);
                }
            }
        }
    }

    #[test]
    fn contrast_extremes() {
        assert!((contrast(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.0).abs() < 1e-9);
        assert!((contrast(Rgb(9, 9, 9), Rgb(9, 9, 9)) - 1.0).abs() < 1e-9);
    }
}
