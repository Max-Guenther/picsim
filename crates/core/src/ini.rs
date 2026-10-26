//! Lesen und Schreiben der picsimu.ini, ohne unbekannte Einträge zu verlieren.

use crate::pins::PinRef;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ini {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    pub fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut current: Option<usize> = None;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = Some(ini.section_index(name));
                continue;
            }
            if let (Some(idx), Some((k, v))) = (current, line.split_once('=')) {
                ini.sections[idx].1.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        ini
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self::parse(&crate::lst::decode_text(&bytes)))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_string())
    }

    fn section_index(&mut self, name: &str) -> usize {
        if let Some(i) = self.sections.iter().position(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return i;
        }
        self.sections.push((name.to_string(), Vec::new()));
        self.sections.len() - 1
    }

    /// Alle Einträge eines Abschnitts.
    pub fn entries(&self, section: &str) -> &[(String, String)] {
        self.sections
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(section))
            .map(|(_, e)| e.as_slice())
            .unwrap_or(&[])
    }

    /// Abschnitt samt allen Einträgen entfernen.
    pub fn remove_section(&mut self, section: &str) {
        self.sections.retain(|(name, _)| !name.eq_ignore_ascii_case(section));
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(section))?
            .1
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, section: &str, key: &str, value: impl Into<String>) {
        let idx = self.section_index(section);
        let entries = &mut self.sections[idx].1;
        let value = value.into();
        match entries.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(e) => e.1 = value,
            None => entries.push((key.to_string(), value)),
        }
    }

    pub fn pin(&self, section: &str, key: &str) -> PinRef {
        self.get(section, key).map(PinRef::parse).unwrap_or_default()
    }

    pub fn set_pin(&mut self, section: &str, key: &str, pin: PinRef) {
        self.set(section, key, pin.to_string());
    }

    pub fn flag(&self, section: &str, key: &str) -> bool {
        self.get(section, key).map(|v| v.trim() == "1").unwrap_or(false)
    }

    pub fn set_flag(&mut self, section: &str, key: &str, on: bool) {
        self.set(section, key, if on { "1" } else { "0" });
    }

    /// Zahl mit Punkt oder deutschem Komma.
    pub fn number(&self, section: &str, key: &str) -> Option<f64> {
        let v = self.get(section, key)?;
        let num: String = v
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.' || *c == '-')
            .collect();
        num.replace(',', ".").parse().ok()
    }

    /// Quarzfrequenz aus `[SETTINGS] XTAL=4,000000 MHz` in Hz.
    pub fn xtal_hz(&self) -> f64 {
        self.number("SETTINGS", "XTAL").map(|mhz| mhz * 1e6).unwrap_or(4e6)
    }

    pub fn set_xtal_hz(&mut self, hz: f64) {
        let s = format!("{:.6} MHz", hz / 1e6).replace('.', ",");
        self.set("SETTINGS", "XTAL", s);
    }
}

impl std::fmt::Display for Ini {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, (name, entries)) in self.sections.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            writeln!(f, "[{name}]")?;
            for (k, v) in entries {
                writeln!(f, "{k}={v}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_section_ignores_case() {
        let mut ini = Ini::parse("[A]\nx=1\n[Files]\nNAME=/tmp/a\n");
        ini.remove_section("FILES");
        assert_eq!(ini.get("FILES", "NAME"), None);
        assert_eq!(ini.get("A", "x"), Some("1"));
    }

    const ORIGINAL: &str = include_str!("../tests/picsimu.ini");

    #[test]
    fn reads_original_ini() {
        let ini = Ini::parse(ORIGINAL);
        assert_eq!(ini.pin("LED_ARRAY", "LED0"), PinRef::port(1, 0));
        assert_eq!(ini.pin("LED_ARRAY", "LED8"), PinRef::Nc);
        assert_eq!(ini.pin("Getraenke", "Muenze1"), PinRef::port(2, 5));
        assert_eq!(ini.xtal_hz(), 4e6);
        assert_eq!(ini.number("FGEN_0", "FREQ"), Some(1.0));
        assert!(ini.flag("LCD", "FORMAT_4"));
    }

    #[test]
    fn roundtrip_keeps_everything() {
        let ini = Ini::parse(ORIGINAL);
        let again = Ini::parse(&ini.to_string());
        assert_eq!(ini, again);
    }

    #[test]
    fn xtal_uses_german_comma() {
        let mut ini = Ini::default();
        ini.set_xtal_hz(20e6);
        assert_eq!(ini.get("SETTINGS", "XTAL"), Some("20,000000 MHz"));
        assert_eq!(ini.xtal_hz(), 20e6);
    }
}
