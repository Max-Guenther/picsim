//! Einlesen von LST-Dateien im Format von iL_ASS16.
//!
//! ```text
//! 0000 1683           00017           bsf status, rp0
//! ^^^^ ^^^^           ^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^
//! Adr. Opcode         Zeile  Quelltext
//! ```
//! Nur Adresse und Opcode sind für die Simulation wichtig, der Rest wird
//! angezeigt.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingLine {
    pub text: String,
    /// Adresse und Opcode, falls die Zeile Code erzeugt.
    pub code: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Default)]
pub struct Listing {
    pub lines: Vec<ListingLine>,
    /// WDT laut CONFIG im Quelltext, falls angegeben.
    pub wdt: Option<bool>,
    pub device: Option<String>,
}

impl Listing {
    pub fn parse(text: &str) -> Self {
        let mut listing = Listing::default();
        for raw in text.lines() {
            let line = raw.trim_end_matches('\r').to_string();
            let code = parse_code(&line);
            let src = source_part(&line).to_ascii_lowercase();
            let src = src.split(';').next().unwrap_or("");
            let mut words = src.split_whitespace();
            match words.next() {
                Some("config") => {
                    for w in words.flat_map(|w| w.split(',')) {
                        match w.trim() {
                            "wdt_on" => listing.wdt = Some(true),
                            "wdt_off" => listing.wdt = Some(false),
                            _ => {}
                        }
                    }
                }
                Some("device") => {
                    listing.device = words.next().map(|d| d.to_ascii_uppercase());
                }
                _ => {}
            }
            listing.lines.push(ListingLine { text: line, code });
        }
        listing
    }

    /// Liest eine Datei, auch in Latin-1 (wie vom alten Assembler erzeugt).
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self::parse(&decode_text(&bytes)))
    }

    pub fn program(&self) -> Vec<(u16, u16)> {
        self.lines.iter().filter_map(|l| l.code).collect()
    }

    /// Zeilenindex des Befehls an einer Programmadresse.
    pub fn line_of(&self, addr: u16) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| matches!(l.code, Some((a, _)) if a == addr))
    }
}

/// UTF-8 wenn gültig, sonst Latin-1.
pub fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

fn parse_code(line: &str) -> Option<(u16, u16)> {
    let b = line.as_bytes();
    if b.len() < 9 || b[4] != b' ' {
        return None;
    }
    let addr = u16::from_str_radix(line.get(0..4)?, 16).ok()?;
    let op = u16::from_str_radix(line.get(5..9)?, 16).ok()?;
    Some((addr, op))
}

/// Quelltext ab Spalte 28 (nach der Zeilennummer).
fn source_part(line: &str) -> &str {
    line.get(27..).unwrap_or("")
}

/// Hängt ".lst" an, wenn die Endung fehlt (wie im Original).
pub fn with_default_extension(path: &str) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(path);
    if p.extension().is_none() {
        p.with_extension("lst")
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        [
            "                    00010           device 16f84",
            "                    00011           config wdt_on",
            "0000 1683           00017           bsf status, rp0",
            "0001 1005           00018           bcf trisa,led",
            "                    00020  loop     ",
            "0003 1806           00021           btfsc portb,key",
        ]
        .join("\r\n")
    }

    #[test]
    fn parses_code_lines() {
        let l = Listing::parse(&sample());
        assert_eq!(l.program(), vec![(0, 0x1683), (1, 0x1005), (3, 0x1806)]);
        assert_eq!(l.line_of(3), Some(5));
        assert_eq!(l.device.as_deref(), Some("16F84"));
        assert_eq!(l.wdt, Some(true));
    }

    #[test]
    fn default_extension() {
        assert_eq!(with_default_extension("testprog1").to_str(), Some("testprog1.lst"));
        assert_eq!(with_default_extension("a/b.LST").to_str(), Some("a/b.LST"));
    }
}
