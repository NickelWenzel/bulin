//! Color math for editing color uniforms: HSV and hex codes. Channels are floats in `0..=1`.

/// A color as hue, saturation and value, each in `0..=1`. A hue of 0 and 1 is the same red.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}

impl Hsv {
    /// The HSV of an RGB color. Greys, black among them, have hue 0 and saturation 0.
    pub fn from_rgb((r, g, b): (f32, f32, f32)) -> Self {
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let hue = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };

        Self {
            hue,
            saturation: if max == 0.0 { 0.0 } else { delta / max },
            value: max,
        }
    }

    /// The RGB color, with channels in `0..=1` when hue, saturation and value are.
    pub fn to_rgb(self) -> (f32, f32, f32) {
        let Self {
            hue,
            saturation,
            value,
        } = self;
        let sector = (hue * 6.0).rem_euclid(6.0);
        let fraction = sector.fract();
        let p = value * (1.0 - saturation);
        let q = value * (1.0 - saturation * fraction);
        let t = value * (1.0 - saturation * (1.0 - fraction));

        match sector as u8 {
            0 => (value, t, p),
            1 => (q, value, p),
            2 => (p, value, t),
            3 => (p, q, value),
            4 => (t, p, value),
            _ => (value, p, q),
        }
    }
}

/// The hex code of a color, `#rrggbb`, or `#rrggbbaa` with an `alpha`.
pub fn hex((r, g, b): (f32, f32, f32), alpha: Option<f32>) -> String {
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    let rgb = format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b));

    match alpha {
        Some(alpha) => format!("{rgb}{:02x}", byte(alpha)),
        None => rgb,
    }
}

/// The color of a hex code `#rgb`, `#rrggbb` or `#rrggbbaa`, the `#` being optional, with the
/// alpha when the code has one.
pub fn parse_hex(text: &str) -> Option<((f32, f32, f32), Option<f32>)> {
    let digits = text.trim();
    let digits = digits.strip_prefix('#').unwrap_or(digits);
    if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |index: usize, width: usize| {
        let byte = u8::from_str_radix(&digits[index * width..(index + 1) * width], 16).ok()?;
        // A short code's digit stands for itself twice: `f` is `ff`.
        let byte = if width == 1 { byte * 17 } else { byte };
        Some(f32::from(byte) / 255.0)
    };

    match digits.len() {
        3 => Some(((channel(0, 1)?, channel(1, 1)?, channel(2, 1)?), None)),
        6 => Some(((channel(0, 2)?, channel(1, 2)?, channel(2, 2)?), None)),
        8 => Some((
            (channel(0, 2)?, channel(1, 2)?, channel(2, 2)?),
            Some(channel(3, 2)?),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close((r, g, b): (f32, f32, f32), (r2, g2, b2): (f32, f32, f32)) -> bool {
        [r - r2, g - g2, b - b2]
            .iter()
            .all(|difference| difference.abs() < 1e-6)
    }

    #[test]
    fn rgb_survives_a_round_trip_through_hsv() {
        for rgb in [
            (1.0, 0.0, 0.0),
            (0.2, 0.8, 0.4),
            (0.1, 0.3, 0.9),
            (0.9, 0.1, 0.7),
            (1.0, 1.0, 0.0),
            (0.5, 0.5, 0.5),
            (0.0, 0.0, 0.0),
        ] {
            assert!(close(Hsv::from_rgb(rgb).to_rgb(), rgb), "{rgb:?}");
        }
    }

    #[test]
    fn primaries_have_their_hues() {
        assert_eq!(Hsv::from_rgb((1.0, 0.0, 0.0)).hue, 0.0);
        assert!((Hsv::from_rgb((0.0, 1.0, 0.0)).hue - 1.0 / 3.0).abs() < 1e-6);
        assert!((Hsv::from_rgb((0.0, 0.0, 1.0)).hue - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_hue_of_one_is_red() {
        let rgb = Hsv {
            hue: 1.0,
            saturation: 1.0,
            value: 1.0,
        }
        .to_rgb();

        assert!(close(rgb, (1.0, 0.0, 0.0)));
    }

    #[test]
    fn grey_and_black_have_no_saturation() {
        let grey = Hsv::from_rgb((0.4, 0.4, 0.4));
        assert_eq!(grey.saturation, 0.0);
        assert_eq!(grey.value, 0.4);

        let black = Hsv::from_rgb((0.0, 0.0, 0.0));
        assert_eq!(black.saturation, 0.0);
        assert_eq!(black.value, 0.0);
    }

    #[test]
    fn hex_codes_survive_a_round_trip() {
        assert_eq!(hex((1.0, 0.5, 0.0), None), "#ff8000");
        assert_eq!(hex((0.0, 0.0, 1.0), Some(0.5)), "#0000ff80");

        for code in ["#ff8000", "#12abef", "#0000ff80"] {
            let (rgb, alpha) = parse_hex(code).unwrap();
            assert_eq!(hex(rgb, alpha), code);
        }
    }

    #[test]
    fn hex_codes_parse_in_every_form() {
        assert_eq!(parse_hex("#fff"), Some(((1.0, 1.0, 1.0), None)));
        assert_eq!(parse_hex("f00"), Some(((1.0, 0.0, 0.0), None)));
        assert_eq!(parse_hex(" 00FF00 "), Some(((0.0, 1.0, 0.0), None)));
        assert_eq!(parse_hex("#000000ff"), Some(((0.0, 0.0, 0.0), Some(1.0))));
    }

    #[test]
    fn malformed_hex_codes_are_refused() {
        for code in [
            "", "#", "#ff", "#ffff", "#fffff", "#fffffff", "#ggg", "##fff", "#ff 00 0",
        ] {
            assert_eq!(parse_hex(code), None, "{code}");
        }
    }
}
