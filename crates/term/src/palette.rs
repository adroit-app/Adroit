#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// The colours a terminal draws with when the program has not set its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub ansi: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            ansi: [
                Rgb::new(0x39, 0x3b, 0x44),
                Rgb::new(0xc9, 0x4f, 0x6d),
                Rgb::new(0x81, 0xb2, 0x9a),
                Rgb::new(0xdb, 0xc0, 0x74),
                Rgb::new(0x71, 0x9c, 0xd6),
                Rgb::new(0x9d, 0x79, 0xd6),
                Rgb::new(0x63, 0xcd, 0xcf),
                Rgb::new(0xdf, 0xdf, 0xe0),
                Rgb::new(0x57, 0x58, 0x60),
                Rgb::new(0xd1, 0x69, 0x83),
                Rgb::new(0x8e, 0xba, 0xa4),
                Rgb::new(0xe0, 0xc9, 0x89),
                Rgb::new(0x86, 0xab, 0xdc),
                Rgb::new(0xba, 0xa1, 0xe2),
                Rgb::new(0x7a, 0xd5, 0xd6),
                Rgb::new(0xe4, 0xe4, 0xe5),
            ],
            foreground: Rgb::new(0xd4, 0xd6, 0xdb),
            background: Rgb::new(0x16, 0x19, 0x1d),
            cursor: Rgb::new(0xe7, 0xe8, 0xeb),
        }
    }
}

impl Rgb {
    /// A colour written `#rrggbb`, or without the `#`.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let at = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self::new(at(0)?, at(2)?, at(4)?))
    }
}

impl Palette {
    /// A palette from a theme in Ghostty's format, `key = value` a line:
    /// `palette = N=#rrggbb` for the sixteen colours, `background`,
    /// `foreground` and `cursor-color`. What it leaves out is as adroit
    /// draws it, but for a cursor, which is then the foreground's colour;
    /// keys about anything else are passed over.
    pub fn from_ghostty(text: &str) -> Result<Self, String> {
        let mut p = Self::default();
        let mut cursor = None;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || format!("line {}: {line:?} is not a colour", n + 1);
            let Some((key, value)) = line.split_once('=') else {
                return Err(bad());
            };
            let colour = |v: &str| Rgb::parse(v).ok_or_else(bad);
            match key.trim() {
                "palette" => {
                    let (index, value) = value.split_once('=').ok_or_else(bad)?;
                    let index: usize = index.trim().parse().map_err(|_| bad())?;
                    *p.ansi.get_mut(index).ok_or_else(bad)? = colour(value)?;
                }
                "background" => p.background = colour(value)?,
                "foreground" => p.foreground = colour(value)?,
                "cursor-color" => cursor = Some(colour(value)?),
                _ => {}
            }
        }
        p.cursor = cursor.unwrap_or(p.foreground);
        Ok(p)
    }

    /// The colour a program asking for colour `index` is told: one of the
    /// 256-colour table, or past it the foreground, the background and the
    /// cursor, in the order the terminal numbers them.
    pub fn query(&self, index: usize) -> Option<Rgb> {
        let c = match index {
            0..=255 => self.indexed(index as u8),
            256 => self.foreground,
            257 => self.background,
            258 => self.cursor,
            _ => return None,
        };
        Some(c)
    }

    /// The colour at an index of the 256-colour table.
    pub fn indexed(&self, index: u8) -> Rgb {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let i = index - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb::new(level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            _ => {
                let v = 8 + (index - 232) * 10;
                Rgb::new(v, v, v)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sixteen_indices_are_the_ansi_colours() {
        let p = Palette::default();
        assert_eq!(p.indexed(1), p.ansi[1]);
        assert_eq!(p.indexed(15), p.ansi[15]);
    }

    #[test]
    fn indices_from_sixteen_form_the_six_level_cube() {
        let p = Palette::default();
        assert_eq!(p.indexed(16), Rgb::new(0, 0, 0));
        assert_eq!(p.indexed(196), Rgb::new(255, 0, 0));
        assert_eq!(p.indexed(231), Rgb::new(255, 255, 255));
    }

    #[test]
    fn the_last_indices_are_a_grey_ramp() {
        let p = Palette::default();
        assert_eq!(p.indexed(232), Rgb::new(8, 8, 8));
        assert_eq!(p.indexed(255), Rgb::new(238, 238, 238));
    }

    #[test]
    fn hex_is_lowercase_with_a_hash() {
        assert_eq!(Rgb::new(0x16, 0xab, 0xff).hex(), "#16abff");
    }

    #[test]
    fn a_theme_in_ghostty_s_format_names_its_colours() {
        let p = Palette::from_ghostty(
            "# a comment\n\
             palette = 0=#393b44\n\
             palette = 15 = E4E4E5\n\
             background = #192330\n\
             foreground=#cdcecf\n\
             cursor-color = #ff0000\n\
             selection-background = #2b3b51\n",
        )
        .unwrap();
        assert_eq!(p.ansi[0], Rgb::new(0x39, 0x3b, 0x44));
        assert_eq!(p.ansi[15], Rgb::new(0xe4, 0xe4, 0xe5));
        assert_eq!(p.background, Rgb::new(0x19, 0x23, 0x30));
        assert_eq!(p.foreground, Rgb::new(0xcd, 0xce, 0xcf));
        assert_eq!(p.cursor, Rgb::new(0xff, 0, 0));
    }

    #[test]
    fn what_a_theme_leaves_out_is_as_adroit_draws_it() {
        let p = Palette::from_ghostty("background = #000000\n").unwrap();
        assert_eq!(p.background, Rgb::new(0, 0, 0));
        assert_eq!(p.ansi, Palette::default().ansi);
        // With no cursor colour of its own, the cursor is the text's.
        let p = Palette::from_ghostty("foreground = #123456\n").unwrap();
        assert_eq!(p.cursor, Rgb::new(0x12, 0x34, 0x56));
    }

    #[test]
    fn a_colour_that_cannot_be_read_is_a_mistake_named_by_its_line() {
        let err = Palette::from_ghostty("background = #19233\n").unwrap_err();
        assert!(err.contains("line 1"), "{err}");
        assert!(Palette::from_ghostty("palette = 16=#000000\n").is_err());
    }
}
