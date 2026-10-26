//! Dateiverknüpfung für Projektdateien (.picsim): Doppelklick im
//! Dateimanager öffnet PicSim mit dem Projekt.
//!
//! - Linux: Dateityp `application/x-picsim` und Starter mit `MimeType` unter
//!   `~/.local/share`, PicSim als Standardprogramm dafür.
//! - Windows: Einträge unter `HKEY_CURRENT_USER\Software\Classes`, keine
//!   Adminrechte nötig. Der Pfad wird bei jedem Start erneuert, falls die
//!   .exe verschoben wurde.
//! - macOS: Dateityp steht in der Info.plist. Geöffnete Dateien kommen nicht
//!   als Argument, sondern als Nachricht an die laufende App (`take_opened`).
//!
//! `register` ändert nur etwas, wenn es noch nicht stimmt.

use std::path::PathBuf;
use std::sync::Mutex;

pub const MIME: &str = "application/x-picsim";

/// Dateien, die das System zum Öffnen übergeben hat (nur macOS).
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REPAINT: Mutex<Option<eframe::egui::Context>> = Mutex::new(None);

#[allow(dead_code)] // nur auf macOS benutzt
fn push_opened(path: PathBuf) {
    if let Ok(mut v) = OPENED.lock() {
        v.push(path);
    }
    if let Some(ctx) = REPAINT.lock().ok().and_then(|c| c.clone()) {
        ctx.request_repaint();
    }
}

/// Seit dem letzten Aufruf übergebene Dateien.
pub fn take_opened() -> Vec<PathBuf> {
    OPENED.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

/// Beim Start aufrufen, sobald das Fenster existiert.
pub fn install(ctx: &eframe::egui::Context) {
    if let Ok(mut c) = REPAINT.lock() {
        *c = Some(ctx.clone());
    }
    #[cfg(target_os = "macos")]
    mac::install();
}

/// Ein Programm direkt aus dem Build-Ordner soll sich nicht als
/// Standardprogramm eintragen, sonst zeigt die Verknüpfung später ins Leere.
#[allow(dead_code)] // auf macOS nicht benutzt
fn installed_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dev = exe.components().any(|c| c.as_os_str() == "target");
    (!dev).then_some(exe)
}

/// Verknüpfung eintragen bzw. auffrischen. Fehler sind nicht schlimm,
/// dann öffnet man Projekte eben über das Menü.
pub fn register() {
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(exe) = installed_exe() {
        linux::register(&exe);
    }
    #[cfg(windows)]
    if let Some(exe) = installed_exe() {
        windows::register(&exe);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::MIME;
    use std::path::Path;

    const MIME_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-picsim">
    <comment>PicSim-Projekt</comment>
    <icon name="picsim"/>
    <glob pattern="*.picsim"/>
  </mime-type>
</mime-info>
"#;

    /// Inhalt des Starters: vorhandenen ergänzen, sonst neu anlegen.
    /// None: schon richtig, nichts zu tun.
    pub fn desktop_entry(existing: Option<&str>, exe: &Path) -> Option<String> {
        let line = format!("MimeType={MIME};");
        match existing {
            Some(text) if text.lines().any(|l| l.starts_with("MimeType=") && l.contains(MIME)) => None,
            Some(text) => {
                let mut out: Vec<String> = text.lines().filter(|l| !l.starts_with("MimeType=")).map(str::to_string).collect();
                for l in &mut out {
                    // Ohne %f bekäme PicSim die Datei nicht übergeben
                    if l.starts_with("Exec=") && !l.contains("%f") && !l.contains("%F") {
                        l.push_str(" %f");
                    }
                }
                let at = out.iter().position(|l| l.trim() == "[Desktop Entry]").map_or(out.len(), |i| i + 1);
                out.insert(at, line);
                Some(out.join("\n") + "\n")
            }
            None => Some(format!(
                "[Desktop Entry]\nType=Application\nName=PicSim\nGenericName=PIC-Simulator\n\
                 Comment=Simulator für den PIC16F84 mit Assembler und Editor\nExec={} %f\nIcon=picsim\n\
                 Terminal=false\nCategories=Development;Education;Electronics;\nStartupWMClass=picsim\n{line}\n",
                exe.display()
            )),
        }
    }

    pub fn register(exe: &Path) {
        let Some(data) = dirs::data_dir() else { return };
        let mut changed = false;

        let xml = data.join("mime/packages/picsim.xml");
        if std::fs::read_to_string(&xml).ok().as_deref() != Some(MIME_XML) {
            let ok = xml.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::write(&xml, MIME_XML).is_ok();
            if ok {
                let _ = std::process::Command::new("update-mime-database").arg(data.join("mime")).status();
                changed = true;
            }
        }

        let desktop = data.join("applications/picsim.desktop");
        let existing = std::fs::read_to_string(&desktop).ok();
        if let Some(text) = desktop_entry(existing.as_deref(), exe) {
            if desktop.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::write(&desktop, text).is_ok() {
                let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).status();
                changed = true;
            }
        }

        if changed {
            let _ = std::process::Command::new("xdg-mime").args(["default", "picsim.desktop", MIME]).status();
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::path::Path;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    const PROG_ID: &str = "PicSim.Projekt";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Standardwert eines Schlüssels setzen. Liefert true, wenn er sich geändert hat.
    fn set_default(path: &str, value: &str) -> bool {
        let mut key: HKEY = null_mut();
        let path = wide(&format!("Software\\Classes\\{path}"));
        // SAFETY: Zeiger auf gültige, nullterminierte Puffer; key wird unten geschlossen
        unsafe {
            if RegCreateKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, null(), REG_OPTION_NON_VOLATILE, KEY_READ | KEY_WRITE, null(), &mut key, null_mut()) != 0 {
                return false;
            }
            let mut buf = [0u16; 1024];
            let mut len = std::mem::size_of_val(&buf) as u32;
            let mut ty = 0;
            let same = RegQueryValueExW(key, null(), null(), &mut ty, buf.as_mut_ptr().cast(), &mut len) == 0
                && ty == REG_SZ
                && String::from_utf16_lossy(&buf[..len as usize / 2]).trim_end_matches('\0') == value;
            if !same {
                let w = wide(value);
                RegSetValueExW(key, null(), 0, REG_SZ, w.as_ptr().cast(), (w.len() * 2) as u32);
            }
            RegCloseKey(key);
            !same
        }
    }

    pub fn register(exe: &Path) {
        let exe = exe.display().to_string();
        let mut changed = false;
        changed |= set_default(".picsim", PROG_ID);
        changed |= set_default(PROG_ID, "PicSim-Projekt");
        changed |= set_default(&format!("{PROG_ID}\\DefaultIcon"), &format!("\"{exe}\",0"));
        changed |= set_default(&format!("{PROG_ID}\\shell\\open\\command"), &format!("\"{exe}\" \"%1\""));
        if changed {
            // Explorer neu einlesen lassen, damit Symbol und Doppelklick sofort stimmen
            // SAFETY: dokumentierter Aufruf ohne Zeigerargumente
            unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as _, SHCNF_IDLIST, null(), null()) };
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSApplication, NSApplicationDelegate};
    use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSURL};

    define_class!(
        // SAFETY: NSObject hat keine Anforderungen an Unterklassen, kein Drop.
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        struct OpenDelegate;

        unsafe impl NSObjectProtocol for OpenDelegate {}

        unsafe impl NSApplicationDelegate for OpenDelegate {
            // SAFETY: Signatur wie in NSApplicationDelegate
            #[unsafe(method(application:openURLs:))]
            fn open_urls(&self, _app: &NSApplication, urls: &NSArray<NSURL>) {
                for url in urls.iter() {
                    if let Some(path) = url.path() {
                        super::push_opened(path.to_string().into());
                    }
                }
            }
        }
    );

    impl OpenDelegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(());
            // SAFETY: init von NSObject
            unsafe { msg_send![super(this), init] }
        }
    }

    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let delegate = OpenDelegate::new(mtm);
        let app = NSApplication::sharedApplication(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // NSApplication hält den Delegate nur schwach: für die ganze Laufzeit behalten
        std::mem::forget(delegate);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn opened_files_are_taken_once() {
        super::push_opened("/tmp/a.picsim".into());
        assert_eq!(super::take_opened(), [std::path::PathBuf::from("/tmp/a.picsim")]);
        assert!(super::take_opened().is_empty());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn desktop_entry_gets_mime_type() {
        use super::linux::desktop_entry;
        let exe = std::path::Path::new("/home/x/.local/bin/picsim");
        let old = "[Desktop Entry]\nType=Application\nName=PicSim\nExec=/home/x/.local/bin/picsim\nIcon=picsim\n";
        let new = desktop_entry(Some(old), exe).unwrap();
        assert!(new.contains("MimeType=application/x-picsim;"));
        assert!(new.contains("Exec=/home/x/.local/bin/picsim %f"));
        assert!(new.contains("Name=PicSim"), "Rest bleibt erhalten");
        assert_eq!(desktop_entry(Some(&new), exe), None, "schon eingetragen: nichts ändern");

        let fresh = desktop_entry(None, exe).unwrap();
        assert!(fresh.contains("Exec=/home/x/.local/bin/picsim %f") && fresh.contains("MimeType=application/x-picsim;"));
    }
}
