//! Shared bits for the app and the Quick Look extension.

pub const APP_BUNDLE_ID: &str = "com.jamzu98.NetherFileOpener";
/// `NSUserDefaults` / CFPreferences key: `true` = white background.
pub const PREF_LIGHT: &str = "LightMode";
/// `NSUserDefaults` key for the font size in points.
pub const PREF_SIZE: &str = "FontSize";

pub const FONT_TTF: &[u8] = include_bytes!("../../../assets/Px437_IBM_VGA_8x16.ttf");
pub const FONT_NAME: &str = "Px437 IBM VGA 8x16";

/// Pixel-perfect sizes for the 8x16 font (1x..4x on Retina when ≥ 8).
pub const SIZES: [f64; 4] = [8.0, 16.0, 24.0, 32.0];
pub const DEFAULT_SIZE: f64 = 16.0;

/// CP437 bytes 0x00-0x1F and 0x7F-0xFF as Unicode.
const LOW: [char; 32] = [
    '\u{0000}', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', //
    '►', '◄', '↕', '‼', '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];
const HIGH: [char; 129] = [
    '⌂', //
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{00A0}',
];

fn cp437(b: u8) -> char {
    match b {
        b'\n' | b'\r' | b'\t' => b as char,
        0x00 | 0x1A => ' ', // NUL and the DOS EOF marker
        0x01..=0x1F => LOW[b as usize],
        0x20..=0x7E => b as char,
        _ => HIGH[(b - 0x7F) as usize],
    }
}

/// Decode NFO bytes: UTF-8 if it is valid and contains non-ASCII, else CP437.
/// Line endings are normalised to `\n`.
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = match std::str::from_utf8(bytes) {
        Ok(s) if !s.is_ascii() => s.replace('\u{1A}', " "),
        _ => bytes.iter().map(|&b| cp437(b)).collect(),
    };
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim_end_matches(['\n', ' ']);
    if !text.contains('\t') {
        return text.to_string();
    }
    // Expand tabs to 8-column stops so columns line up.
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut col = 0;
        for c in line.chars() {
            if c == '\t' {
                let n = 8 - col % 8;
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            } else {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

/// Width in columns and height in lines of decoded text.
pub fn dimensions(text: &str) -> (usize, usize) {
    let cols = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    (cols, text.lines().count().max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp437_blocks_and_boxes() {
        assert_eq!(decode(&[0xDB, 0xB0, 0xB1, 0xB2, 0xC9, 0xCD, 0xBB]), "█░▒▓╔═╗");
        assert_eq!(decode(&[0x7F, 0xFE, 0x01]), "⌂■☺");
    }

    #[test]
    fn utf8_passthrough_and_newlines() {
        assert_eq!(decode("╔═╗\r\n║█║\r\n".as_bytes()), "╔═╗\n║█║");
        assert_eq!(decode(b"plain\r\nascii"), "plain\nascii");
    }

    #[test]
    fn tabs_expand() {
        assert_eq!(decode(b"a\tb"), "a       b");
    }

    #[test]
    fn dims() {
        assert_eq!(dimensions("ab\nabcd\n"), (4, 2));
    }
}
