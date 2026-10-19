//! Updater über GitHub-Releases.
//!
//! Ablauf: neuestes Release abfragen, Version vergleichen, passendes Paket
//! laden, SHA-256 prüfen, Programmdatei austauschen. Danach Neustart.

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub const REPO: &str = "Max-Guenther/picsim";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Release {
    pub version: Version,
    pub notes: String,
    pub page: String,
    pub asset_url: String,
    pub sha_url: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Name des Release-Pakets für dieses System.
pub fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("picsim-linux-x86_64.tar.gz"),
        ("macos", "aarch64") => Some("picsim-macos-aarch64.tar.gz"),
        _ => None,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(concat!("picsim/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Liefert das neueste Release, wenn es neuer als die laufende Version ist.
pub fn check() -> Result<Option<Release>, String> {
    let current = Version::parse(CURRENT).expect("eigene Version ist gültig");
    Ok(latest()?.filter(|r| r.version > current))
}

/// Das neueste veröffentlichte Release mit Paket für dieses System.
fn latest() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let gh: GhRelease = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub nicht erreichbar: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Antwort von GitHub unlesbar: {e}"))?;
    if gh.draft || gh.prerelease {
        return Ok(None);
    }
    let version = Version::parse(gh.tag_name.trim_start_matches('v'))
        .map_err(|e| format!("Unbekanntes Versionsformat {}: {e}", gh.tag_name))?;
    let name = asset_name().ok_or("Für dieses System gibt es kein fertiges Paket")?;
    let asset = gh
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("Im Release {} fehlt das Paket {name}", gh.tag_name))?;
    let sha_url = gh
        .assets
        .iter()
        .find(|a| a.name == format!("{name}.sha256"))
        .map(|a| a.browser_download_url.clone());
    Ok(Some(Release {
        version,
        notes: gh.body.unwrap_or_default(),
        page: gh.html_url,
        asset_url: asset.browser_download_url.clone(),
        sha_url,
    }))
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    agent()
        .get(url)
        .call()
        .map_err(|e| format!("Download fehlgeschlagen: {e}"))?
        .body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("Download abgebrochen: {e}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Lädt das Paket, prüft die Prüfsumme und holt die Programmdatei heraus.
fn fetch_binary(rel: &Release) -> Result<Vec<u8>, String> {
    let data = download(&rel.asset_url)?;
    let sha_url = rel.sha_url.as_ref().ok_or("Im Release fehlt die Prüfsumme, Update abgebrochen")?;
    let expected = String::from_utf8_lossy(&download(sha_url)?)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if expected != hex(&Sha256::digest(&data)) {
        return Err("Prüfsumme stimmt nicht, Update abgebrochen".into());
    }

    let gz = flate2::read::GzDecoder::new(&data[..]);
    let mut archive = tar::Archive::new(gz);
    let mut binary = None;
    for entry in archive.entries().map_err(|e| format!("Paket defekt: {e}"))? {
        let mut entry = entry.map_err(|e| format!("Paket defekt: {e}"))?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        if path.file_name().is_some_and(|n| n == "picsim") && entry.header().entry_type().is_file() {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| format!("Paket defekt: {e}"))?;
            binary = Some(buf);
            break;
        }
    }
    binary.ok_or_else(|| "Im Paket fehlt die Programmdatei".to_string())
}

/// Lädt das Release und ersetzt die laufende Programmdatei.
pub fn install(rel: &Release) -> Result<(), String> {
    let binary = fetch_binary(rel)?;
    let tmp: PathBuf = std::env::temp_dir().join(format!("picsim-update-{}", rel.version));
    std::fs::write(&tmp, &binary).map_err(|e| format!("Kann nicht schreiben: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let res = self_replace::self_replace(&tmp).map_err(|e| format!("Programmdatei lässt sich nicht ersetzen: {e}"));
    let _ = std::fs::remove_file(&tmp);
    res?;
    #[cfg(target_os = "macos")]
    resign_bundle();
    Ok(())
}

/// Nach dem Austausch passt die Ad-hoc-Signatur der App nicht mehr zum
/// Inhalt. Neu signieren, damit macOS die App weiter startet.
#[cfg(target_os = "macos")]
fn resign_bundle() {
    let Ok(exe) = std::env::current_exe() else { return };
    if let Some(app) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
        let _ = std::process::Command::new("codesign").args(["--force", "--deep", "-s", "-"]).arg(app).status();
    }
}

/// Startet das Programm neu (nach einem Update).
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let _ = std::process::Command::new(exe).args(args).spawn();
    }
    std::process::exit(0);
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(Version),
    Installed(Version),
    Failed(String),
}

enum Msg {
    Checked(Result<Option<Release>, String>),
    Installed(Result<Version, String>),
}

/// Prüfung oder Installation im Hintergrund.
pub struct Updater {
    pub state: UpdateState,
    /// Ergebnis soll sichtbar gemeldet werden (Prüfung von Hand gestartet).
    pub manual: bool,
    rx: Option<Receiver<Msg>>,
}

impl Default for Updater {
    fn default() -> Self {
        Updater { state: UpdateState::Idle, manual: false, rx: None }
    }
}

impl Updater {
    pub fn check(&mut self, ctx: &eframe::egui::Context, manual: bool) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Checked(check()));
            ctx.request_repaint();
        });
        self.state = UpdateState::Checking;
        self.manual = manual;
        self.rx = Some(rx);
    }

    pub fn install(&mut self, rel: Release, ctx: &eframe::egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.state = UpdateState::Installing(rel.version.clone());
        std::thread::spawn(move || {
            let res = install(&rel).map(|_| rel.version.clone());
            let _ = tx.send(Msg::Installed(res));
            ctx.request_repaint();
        });
        self.rx = Some(rx);
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        if let Ok(msg) = rx.try_recv() {
            self.state = match msg {
                Msg::Checked(Ok(Some(r))) => UpdateState::Available(r),
                Msg::Checked(Ok(None)) => UpdateState::UpToDate,
                Msg::Checked(Err(e)) | Msg::Installed(Err(e)) => UpdateState::Failed(e),
                Msg::Installed(Ok(v)) => UpdateState::Installed(v),
            };
            self.rx = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_for_this_system() {
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert_eq!(asset_name(), Some("picsim-linux-x86_64.tar.gz"));
        }
    }

    /// Lädt das echte, neueste Release von GitHub. Braucht Netz, daher nur
    /// auf Wunsch: cargo test -p picsim -- --ignored
    #[test]
    #[ignore]
    fn download_latest_release() {
        let rel = latest().unwrap().expect("es gibt ein Release");
        let bin = fetch_binary(&rel).unwrap();
        if cfg!(target_os = "linux") {
            assert_eq!(&bin[..4], b"\x7fELF");
        }
        assert!(bin.len() > 1_000_000);
    }

    #[test]
    fn sha_hex() {
        assert_eq!(hex(&Sha256::digest(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}
