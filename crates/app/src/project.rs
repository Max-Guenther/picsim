//! Projektdateien (.picsim): ein Zip mit allem, was zum Projekt gehört,
//! damit es auf einem anderen Rechner genauso läuft.
//!
//! Inhalt: `projekt.ini` (alle Einstellungen außer den rechnereigenen,
//! dazu Schalterstellungen und Haltepunkte) und der Quelltext mit allen
//! INCLUDE-Dateien unter ihrem Pfad relativ zur Quelldatei. Ohne Quelltext
//! liegt stattdessen das Listing darin. Geöffnet wird ein Projekt in einen
//! Arbeitsordner im Cache, dort landen auch LST, HEX usw.

use picsim_core::Ini;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const EXT: &str = "picsim";
const INI_NAME: &str = "projekt.ini";
pub const SEC: &str = "PROJEKT";
pub const STATE: &str = "ZUSTAND";
/// Abschnitte, die zu diesem Rechner gehören und nicht ins Projekt.
const LOCAL: [&str; 3] = ["FILES", "UPDATE", "DISPLAY"];

pub struct Project {
    pub ini: Ini,
    /// Dateien mit Pfad relativ zum Projekt.
    pub files: Vec<(String, Vec<u8>)>,
}

impl Project {
    /// Projekt aus den aktuellen Einstellungen und Dateien.
    pub fn new(settings: &Ini) -> Self {
        let mut ini = settings.clone();
        for sec in LOCAL {
            ini.remove_section(sec);
        }
        ini.remove_section(SEC);
        ini.set(SEC, "Format", "1");
        Project { ini, files: Vec::new() }
    }

    /// Quelltext (Inhalt `bytes`, gelesen aus `dir`) samt Includes aufnehmen.
    pub fn add_source(&mut self, name: &str, bytes: Vec<u8>, dir: Option<&Path>) -> Result<(), String> {
        let text = picsim_core::lst::decode_text(&bytes);
        for inc in picsim_asm::include_files(&text, dir) {
            let rel = dir
                .and_then(|d| inc.strip_prefix(d).ok())
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .filter(|r| !r.starts_with(".."))
                .ok_or_else(|| format!("{} liegt nicht im Ordner der Quelldatei und kann nicht ins Projekt", inc.display()))?;
            let data = std::fs::read(&inc).map_err(|e| format!("{}: {e}", inc.display()))?;
            self.files.push((rel, data));
        }
        self.files.push((name.to_string(), bytes));
        self.ini.set(SEC, "Quelltext", name);
        Ok(())
    }

    /// Nur ein Listing aufnehmen (kein Quelltext vorhanden).
    pub fn add_listing(&mut self, path: &Path) -> io::Result<()> {
        let name = file_name(path);
        self.files.push((name.clone(), std::fs::read(path)?));
        self.ini.set(SEC, "Listing", name);
        Ok(())
    }

    pub fn source(&self) -> Option<&str> {
        self.ini.get(SEC, "Quelltext")
    }

    pub fn listing(&self) -> Option<&str> {
        self.ini.get(SEC, "Listing")
    }

    /// Einstellungen des Projekts, die rechnereigenen aus `local` behalten.
    pub fn settings(&self, local: &Ini) -> Ini {
        let mut ini = self.ini.clone();
        ini.remove_section(SEC);
        ini.remove_section(STATE);
        for sec in LOCAL {
            ini.remove_section(sec);
            for (k, v) in local.entries(sec) {
                ini.set(sec, k, v.as_str());
            }
        }
        ini
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        // Erst daneben schreiben, dann umbenennen: ein Fehler zerstört nicht das alte Projekt
        let tmp = path.with_extension("picsim.tmp");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&tmp)?);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file(INI_NAME, opts).map_err(io::Error::other)?;
            zip.write_all(self.ini.to_string().as_bytes())?;
            for (name, data) in &self.files {
                zip.start_file(name.as_str(), opts).map_err(io::Error::other)?;
                zip.write_all(data)?;
            }
            zip.finish().map_err(io::Error::other)?;
        }
        std::fs::rename(&tmp, path)
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        let bad = |msg: &str| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {msg}", path.display()));
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| bad("keine PicSim-Projektdatei"))?;
        let mut ini = None;
        let mut files = Vec::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(io::Error::other)?;
            if entry.is_dir() {
                continue;
            }
            // Pfade mit ".." oder absolute Pfade nie entpacken
            let Some(name) = entry.enclosed_name() else { continue };
            let name = name.to_string_lossy().replace('\\', "/");
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            if name == INI_NAME {
                ini = Some(Ini::parse(&picsim_core::lst::decode_text(&data)));
            } else {
                files.push((name, data));
            }
        }
        let ini = ini.ok_or_else(|| bad("projekt.ini fehlt"))?;
        Ok(Project { ini, files })
    }

    /// Dateien in `dir` entpacken. Der Ordner wird vorher geleert.
    pub fn extract(&self, dir: &Path) -> io::Result<()> {
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        std::fs::create_dir_all(dir)?;
        for (name, data) in &self.files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, data)?;
        }
        Ok(())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Arbeitsordner für ein geöffnetes Projekt unter `cache`, eindeutig je Projektdatei.
pub fn workdir(project: &Path, cache: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let full = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    full.hash(&mut h);
    let stem = project.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    cache.join("projekte").join(format!("{stem}-{:08x}", h.finish() as u32))
}

/// Schalterstellungen als Text, z.B. "HOL…" (High, offen, Low).
pub fn switches_to_text(pos: &[picsim_core::board::SwitchPos]) -> String {
    use picsim_core::board::SwitchPos::*;
    pos.iter().map(|p| match p {
        High => 'H',
        Open => 'O',
        Low => 'L',
    }).collect()
}

pub fn switches_from_text(text: &str, pos: &mut [picsim_core::board::SwitchPos]) {
    use picsim_core::board::SwitchPos::*;
    for (p, c) in pos.iter_mut().zip(text.chars()) {
        *p = match c {
            'H' => High,
            'L' => Low,
            _ => Open,
        };
    }
}

pub fn breakpoints_to_text(bp: &std::collections::BTreeSet<u16>) -> String {
    bp.iter().map(|a| format!("{a:04X}")).collect::<Vec<_>>().join(";")
}

pub fn breakpoints_from_text(text: &str) -> std::collections::BTreeSet<u16> {
    text.split(';').filter_map(|a| u16::from_str_radix(a.trim(), 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use picsim_core::board::SwitchPos;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("picsim-projekt-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_with_includes() {
        let dir = tmp("rt");
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/regs.inc"), "status equ 3\n").unwrap();
        let src = b"\tinclude \"lib/regs.inc\"\n\tgoto 0\n".to_vec();

        let mut settings = Ini::parse("[LED_ARRAY]\nLED0=RB3\n[FILES]\nNAME=/home/x/a.lst\n[UPDATE]\nAUTO=0\n");
        settings.set("EDITOR", "TABBREITE", "8");
        let mut p = Project::new(&settings);
        p.add_source("main.src", src.clone(), Some(&dir)).unwrap();
        p.ini.set(STATE, "Schalter", "HL");
        let file = dir.join("test.picsim");
        p.write(&file).unwrap();

        let q = Project::read(&file).unwrap();
        assert_eq!(q.source(), Some("main.src"));
        assert_eq!(q.ini.get("FILES", "NAME"), None, "Pfade dieses Rechners gehören nicht ins Projekt");
        assert_eq!(q.ini.get("LED_ARRAY", "LED0"), Some("RB3"));

        // Beim Kollegen: seine eigenen Pfade und Update-Einstellung bleiben
        let local = Ini::parse("[UPDATE]\nAUTO=1\n[LED_ARRAY]\nLED0=RA0\n");
        let merged = q.settings(&local);
        assert_eq!(merged.get("UPDATE", "AUTO"), Some("1"));
        assert_eq!(merged.get("LED_ARRAY", "LED0"), Some("RB3"));
        assert_eq!(merged.get("EDITOR", "TABBREITE"), Some("8"));
        assert_eq!(merged.get(STATE, "Schalter"), None);

        let work = dir.join("arbeit");
        q.extract(&work).unwrap();
        assert_eq!(std::fs::read(work.join("main.src")).unwrap(), src);
        assert_eq!(std::fs::read_to_string(work.join("lib/regs.inc")).unwrap(), "status equ 3\n");
        // Der entpackte Quelltext lässt sich übersetzen
        let (out, _) = picsim_asm::assemble_file(&work.join("main.src")).unwrap();
        assert!(out.ok(), "{:?}", out.errors);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn include_outside_folder_is_refused() {
        let dir = tmp("aussen");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("x.inc"), "x equ 1\n").unwrap();
        let mut p = Project::new(&Ini::default());
        let err = p.add_source("a.src", b"\tinclude \"../x.inc\"\n".to_vec(), Some(&dir.join("sub"))).unwrap_err();
        assert!(err.contains("nicht im Ordner"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn evil_paths_are_not_extracted() {
        let dir = tmp("boese");
        let file = dir.join("boese.picsim");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            zip.start_file("projekt.ini", o).unwrap();
            zip.write_all(b"[PROJEKT]\nFormat=1\n").unwrap();
            zip.start_file("../../ausbruch.txt", o).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        let p = Project::read(&file).unwrap();
        assert!(p.files.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn not_a_project() {
        let dir = tmp("kaputt");
        let file = dir.join("x.picsim");
        std::fs::write(&file, "kein zip").unwrap();
        assert!(Project::read(&file).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn state_text_roundtrip() {
        let mut pos = [SwitchPos::Open; 16];
        pos[0] = SwitchPos::High;
        pos[15] = SwitchPos::Low;
        let text = switches_to_text(&pos);
        assert_eq!(text, "HOOOOOOOOOOOOOOL");
        let mut back = [SwitchPos::Open; 16];
        switches_from_text(&text, &mut back);
        assert_eq!(back, pos);

        let bp: std::collections::BTreeSet<u16> = [4, 0x10].into();
        assert_eq!(breakpoints_to_text(&bp), "0004;0010");
        assert_eq!(breakpoints_from_text("0004;0010;zz"), bp);
    }
}
