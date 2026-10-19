//! Applikationen: Ampelkreuzung und Getränkeautomat.

mod ampel;
mod getraenke;

use eframe::egui;
use picsim_core::{Board, Ini};

#[derive(Default)]
pub struct Apps {
    pub ampel: ampel::Ampel,
    pub getraenke: getraenke::Getraenke,
    last_us: f64,
}

impl Apps {
    pub fn load(&mut self, ini: &Ini) {
        self.ampel.load(ini);
        self.getraenke.load(ini);
        self.ampel.open = ini.flag("Applikationen", "Ampel");
        self.getraenke.open = ini.flag("Applikationen", "Getraenke");
    }

    pub fn store(&self, ini: &mut Ini) {
        self.ampel.store(ini);
        self.getraenke.store(ini);
        ini.set_flag("Applikationen", "Ampel", self.ampel.open);
        ini.set_flag("Applikationen", "Getraenke", self.getraenke.open);
    }

    pub fn menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("Ampeln an einer Kreuzung").clicked() {
            self.ampel.open = true;
        }
        if ui.button("Getränkeautomat").clicked() {
            self.getraenke.open = true;
        }
    }

    /// Bewegt die offenen Applikationen um die seit dem letzten Bild
    /// vergangene Simulationszeit weiter.
    pub fn tick(&mut self, board: &mut Board, ctx: &egui::Context) {
        let now = board.cpu.runtime_us();
        // Nach Reset oder zurückgesetzter Laufzeit beginnt die Zeit neu
        let dt = if now >= self.last_us { ((now - self.last_us) / 1e6) as f32 } else { 0.0 };
        self.last_us = now;
        let dt = dt.min(1.0);
        if self.ampel.open {
            self.ampel.tick(board, dt);
        }
        if self.getraenke.open {
            self.getraenke.tick(board, dt);
        }
        if (self.ampel.open || self.getraenke.open) && board.running() {
            ctx.request_repaint();
        }
    }

    /// Zeigt offene Fenster. Liefert true, wenn Einstellungen geändert wurden.
    pub fn show(&mut self, ctx: &egui::Context, board: &mut Board) -> bool {
        let a = self.ampel.show(ctx, board);
        let g = self.getraenke.show(ctx, board);
        a || g
    }
}
