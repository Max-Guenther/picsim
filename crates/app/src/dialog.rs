//! Dateidialoge, die das Programm nicht anhalten.
//!
//! Ein blockierender Dialog (`rfd::FileDialog`) läuft mitten im Zeichnen
//! eines Bildes. Auf macOS verarbeitet er dabei weiter Ereignisse, winit
//! will sie an die gerade beschäftigte App geben und bricht das Programm
//! ab (passiert z.B. bei einer Trackpad-Geste im Speichern-Dialog). Darum
//! werden Dialoge hier nur gestartet und dann jedes Bild abgefragt.

use eframe::egui;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

type DialogFuture = Pin<Box<dyn Future<Output = Option<rfd::FileHandle>> + Send>>;

/// Ein offener Dialog. `purpose` sagt, was mit der gewählten Datei passieren soll.
pub struct FileDialog<T> {
    future: DialogFuture,
    pub purpose: T,
}

impl<T> FileDialog<T> {
    pub fn open(dialog: rfd::AsyncFileDialog, purpose: T) -> Self {
        FileDialog { future: Box::pin(dialog.pick_file()), purpose }
    }

    pub fn save(dialog: rfd::AsyncFileDialog, purpose: T) -> Self {
        FileDialog { future: Box::pin(dialog.save_file()), purpose }
    }
}

/// Neu zeichnen, sobald der Dialog fertig ist, damit das Ergebnis ankommt.
struct Repaint(egui::Context);

impl Wake for Repaint {
    fn wake(self: Arc<Self>) {
        self.0.request_repaint();
    }
}

/// Fragt den Dialog in `slot` ab. Ist er fertig, wird er entfernt und
/// Zweck samt gewählter Datei (None: abgebrochen) zurückgegeben.
pub fn poll<T>(slot: &mut Option<FileDialog<T>>, ctx: &egui::Context) -> Option<(T, Option<PathBuf>)> {
    let dialog = slot.as_mut()?;
    let waker = Waker::from(Arc::new(Repaint(ctx.clone())));
    let Poll::Ready(file) = dialog.future.as_mut().poll(&mut Context::from_waker(&waker)) else {
        return None;
    };
    let dialog = slot.take()?;
    Some((dialog.purpose, file.map(|f| f.path().to_path_buf())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(file: Option<&str>) -> FileDialog<u8> {
        let file = file.map(|f| rfd::FileHandle::from(PathBuf::from(f)));
        FileDialog { future: Box::pin(std::future::ready(file)), purpose: 7 }
    }

    #[test]
    fn finished_dialog_is_taken() {
        let ctx = egui::Context::default();
        let mut slot = Some(ready(Some("/tmp/a.src")));
        assert_eq!(poll(&mut slot, &ctx), Some((7, Some(PathBuf::from("/tmp/a.src")))));
        assert!(slot.is_none());
        assert_eq!(poll(&mut slot, &ctx), None);
    }

    #[test]
    fn cancelled_dialog_gives_no_path() {
        let ctx = egui::Context::default();
        let mut slot = Some(ready(None));
        assert_eq!(poll(&mut slot, &ctx), Some((7, None)));
    }

    #[test]
    fn open_dialog_stays() {
        let ctx = egui::Context::default();
        let mut slot = Some(FileDialog { future: Box::pin(std::future::pending()), purpose: 1u8 });
        assert_eq!(poll(&mut slot, &ctx), None);
        assert!(slot.is_some());
    }
}
