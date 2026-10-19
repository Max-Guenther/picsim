//! Applikationen: Ampelkreuzung und Getränkeautomat.

use eframe::egui;
use picsim_core::Board;

#[derive(Default)]
pub struct Apps {
    pub ampel: bool,
    pub getraenke: bool,
}

impl Apps {
    pub fn menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("Ampeln an einer Kreuzung").clicked() {
            self.ampel = true;
        }
        if ui.button("Getränkeautomat").clicked() {
            self.getraenke = true;
        }
    }

    /// Wird jedes Bild aufgerufen, auch wenn die Fenster zu sind.
    pub fn tick(&mut self, _board: &mut Board, _ctx: &egui::Context) {}

    /// Zeigt offene Fenster. Liefert true, wenn Einstellungen geändert wurden.
    pub fn show(&mut self, _ctx: &egui::Context, _board: &mut Board) -> bool {
        false
    }
}
