//! Applikation "Getränkeautomat".
//!
//! Aus Sicht des PIC sind Münzprüfer, Wahltasten, Leersensoren und
//! Geldrückgabe Eingänge. Motoren, Klappen der Zwischenkasse und die
//! Kreditanzeige sind Ausgänge.

use crate::canvas::Canvas;
use crate::settings::pin_combo;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use picsim_core::devices::level;
use picsim_core::{Board, Ini, PinRef};

const SEC: &str = "Getraenke";
/// So lange liegt der Münzkanal auf High (Simulationszeit).
const COIN_FALL_S: f32 = 0.1;
/// Dauer einer Flaschenausgabe.
const VEND_S: f32 = 0.8;
/// So lange liegt die Geldrückgabe auf High.
const ABBRUCH_S: f32 = 0.3;
const FULL: u8 = 6;

const COINS: [(&str, u32, f32); 3] = [("0,50 €", 50, 13.0), ("1 €", 100, 15.0), ("2 €", 200, 17.0)];
const PRODUCTS: [(&str, &str, Color32); 4] = [
    ("Wasser", "1,00 €", Color32::from_rgb(90, 160, 230)),
    ("Limo", "2,00 €", Color32::from_rgb(240, 200, 40)),
    ("Cola", "2,00 €", Color32::from_rgb(110, 50, 30)),
    ("Bier", "3,50 €", Color32::from_rgb(200, 140, 30)),
];

#[derive(Clone, Copy, Default)]
pub struct Pins {
    pub muenze: [PinRef; 3],
    pub motor: [PinRef; 4],
    pub taster: [PinRef; 4],
    pub sensor: [PinRef; 4],
    pub abbruch: PinRef,
    pub kasse: [PinRef; 2],
    pub kredit: [PinRef; 3],
}

#[derive(Clone, Copy)]
enum Click {
    Product(usize),
    Abbruch,
    Klappe,
}

pub struct Getraenke {
    pub open: bool,
    pub settings: bool,
    pub pins: Pins,
    /// true: erst Produkt wählen, dann bezahlen.
    pub select_first: bool,
    stock: [u8; 4],
    pressed: [bool; 4],
    falling: Vec<(usize, f32)>,
    zwischenkasse: Vec<usize>,
    kasse: Vec<usize>,
    rueckgabe: Vec<usize>,
    bottle_out: Option<usize>,
    vending: [Option<f32>; 4],
    abbruch_t: f32,
    slot: Rect,
    regions: Vec<(Rect, Click)>,
}

impl Default for Getraenke {
    fn default() -> Self {
        Getraenke {
            open: false,
            settings: false,
            pins: Pins::default(),
            select_first: false,
            stock: [FULL; 4],
            pressed: [false; 4],
            falling: Vec::new(),
            zwischenkasse: Vec::new(),
            kasse: Vec::new(),
            rueckgabe: Vec::new(),
            bottle_out: None,
            vending: [None; 4],
            abbruch_t: 0.0,
            slot: Rect::NOTHING,
            regions: Vec::new(),
        }
    }
}

fn euro(cents: u32) -> String {
    format!("{},{:02} €", cents / 100, cents % 100)
}

fn sum(coins: &[usize]) -> u32 {
    coins.iter().map(|&c| COINS[c].1).sum()
}

fn high(pins: &[u8; 5], p: PinRef) -> bool {
    level(pins, p) == Some(true)
}

fn drive(board: &mut Board, p: PinRef, on: bool) {
    if let PinRef::Port { port, bit } = p {
        board.cpu.set_input(port, bit, on);
    }
}

impl Getraenke {
    pub fn load(&mut self, ini: &Ini) {
        let p = &mut self.pins;
        for i in 0..3 {
            p.muenze[i] = ini.pin(SEC, &format!("Muenze{}", i + 1));
            p.kredit[i] = ini.pin(SEC, &format!("Kredit{}", i + 1));
        }
        for i in 0..4 {
            p.motor[i] = ini.pin(SEC, &format!("Motor{}", i + 1));
            p.taster[i] = ini.pin(SEC, &format!("Taster{}", i + 1));
            p.sensor[i] = ini.pin(SEC, &format!("Sensor{}", i + 1));
        }
        p.abbruch = ini.pin(SEC, "Abbruch");
        p.kasse = [ini.pin(SEC, "Kasse1"), ini.pin(SEC, "Kasse2")];
        self.select_first = ini.flag(SEC, "ErstWaehlen");
    }

    pub fn store(&self, ini: &mut Ini) {
        let p = &self.pins;
        for i in 0..3 {
            ini.set_pin(SEC, &format!("Muenze{}", i + 1), p.muenze[i]);
            ini.set_pin(SEC, &format!("Kredit{}", i + 1), p.kredit[i]);
        }
        for i in 0..4 {
            ini.set_pin(SEC, &format!("Motor{}", i + 1), p.motor[i]);
            ini.set_pin(SEC, &format!("Taster{}", i + 1), p.taster[i]);
            ini.set_pin(SEC, &format!("Sensor{}", i + 1), p.sensor[i]);
        }
        ini.set_pin(SEC, "Abbruch", p.abbruch);
        ini.set_pin(SEC, "Kasse1", p.kasse[0]);
        ini.set_pin(SEC, "Kasse2", p.kasse[1]);
        ini.set_flag(SEC, "ErstWaehlen", self.select_first);
    }

    /// Ausgabefach belegt: dann nimmt der Münzprüfer nichts an.
    fn tray_busy(&self) -> bool {
        self.bottle_out.is_some() || !self.rueckgabe.is_empty()
    }

    fn insert_coin(&mut self, c: usize) -> bool {
        if self.tray_busy() {
            return false;
        }
        self.falling.push((c, COIN_FALL_S));
        true
    }

    fn click(&mut self, what: Click, shift: bool) {
        match what {
            Click::Product(i) if shift => self.stock[i] = FULL,
            Click::Product(i) => self.pressed[i] = true,
            Click::Abbruch => {
                self.abbruch_t = ABBRUCH_S;
                self.pressed = [false; 4];
            }
            Click::Klappe => {
                self.bottle_out = None;
                self.rueckgabe.clear();
            }
        }
    }

    /// Ein Zeitschritt in Simulationssekunden.
    pub fn tick(&mut self, board: &mut Board, dt: f32) {
        let pins = board.cpu.pins();

        // Münzen fallen durch den Prüfer in die Zwischenkasse
        for (_, t) in &mut self.falling {
            *t -= dt;
        }
        let (done, rest): (Vec<_>, Vec<_>) = self.falling.drain(..).partition(|(_, t)| *t <= 0.0);
        self.falling = rest;
        let mut channel = [false; 3];
        for (c, _) in &self.falling {
            channel[*c] = true;
        }
        self.zwischenkasse.extend(done.into_iter().map(|(c, _)| c));

        // Klappen der Zwischenkasse
        if high(&pins, self.pins.kasse[0]) {
            self.kasse.append(&mut self.zwischenkasse);
        }
        if high(&pins, self.pins.kasse[1]) {
            self.rueckgabe.append(&mut self.zwischenkasse);
        }

        // Motoren
        for i in 0..4 {
            match self.vending[i] {
                None if high(&pins, self.pins.motor[i]) => {
                    self.vending[i] = Some(VEND_S);
                    self.pressed[i] = false; // Wischerfunktion der Wahltaste
                }
                Some(t) if t - dt <= 0.0 => {
                    self.vending[i] = None;
                    if self.stock[i] > 0 {
                        self.stock[i] -= 1;
                        self.bottle_out = Some(i);
                    }
                    // Motor-Pin nach der Ausgabe zurücksetzen (Wischerfunktion)
                    if let PinRef::Port { port, bit } = self.pins.motor[i] {
                        board.cpu.latch[port as usize] &= !(1 << bit);
                    }
                }
                Some(t) => self.vending[i] = Some(t - dt),
                None => {}
            }
        }
        self.abbruch_t = (self.abbruch_t - dt).max(0.0);

        for (c, on) in channel.iter().enumerate() {
            drive(board, self.pins.muenze[c], *on);
        }
        for i in 0..4 {
            drive(board, self.pins.taster[i], self.pressed[i]);
            drive(board, self.pins.sensor[i], self.stock[i] == 0);
        }
        drive(board, self.pins.abbruch, self.abbruch_t > 0.0);
    }

    pub fn show(&mut self, ctx: &egui::Context, board: &Board) -> bool {
        let mut open = self.open;
        egui::Window::new("Getränkeautomat").order(egui::Order::Foreground)
            .open(&mut open)
            .resizable(true)
            .default_size([520.0, 480.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Münzen").strong());
                        ui.label(RichText::new("in den Schlitz ziehen").small());
                        for c in 0..3 {
                            self.coin_widget(ui, c);
                        }
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("Kasse: {}", euro(sum(&self.kasse)))).strong(),
                        );
                        if ui.small_button("Kasse leeren").clicked() {
                            self.kasse.clear();
                        }
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("Shift+Klick auf eine\nWahltaste füllt den Schacht.")
                                .small(),
                        );
                        ui.add_space(8.0);
                        if ui.button("Einstellungen…").clicked() {
                            self.settings = true;
                        }
                    });
                    self.draw(ui, board);
                });
            });
        self.open = open;
        self.settings_window(ctx)
    }

    fn coin_widget(&mut self, ui: &mut egui::Ui, c: usize) {
        let (name, _, r) = COINS[c];
        let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 40.0), Sense::drag());
        let gold = Color32::from_rgb(210, 175, 60);
        let silver = Color32::from_rgb(190, 190, 195);
        let (outer, inner) = match c {
            0 => (gold, gold),
            1 => (gold, silver),
            _ => (silver, gold),
        };
        let draw = |p: &egui::Painter, at: Pos2| {
            p.circle_filled(at, r, outer);
            p.circle_filled(at, r * 0.7, inner);
            p.text(
                at,
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::proportional(9.0),
                Color32::BLACK,
            );
        };
        draw(ui.painter(), rect.center());
        let resp = resp.on_hover_text(format!("{name}: in den Münzschlitz ziehen"));
        if resp.dragged() {
            if let Some(pos) = ui.ctx().pointer_interact_pos() {
                let layer =
                    egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique(("coin_drag", c)));
                draw(&ui.ctx().layer_painter(layer), pos);
            }
        }
        if resp.drag_stopped() {
            if let Some(pos) = ui.ctx().pointer_interact_pos() {
                if self.slot.expand(12.0).contains(pos) {
                    self.insert_coin(c);
                }
            }
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, board: &Board) {
        let canvas = Canvas::new(ui, vec2(400.0, 460.0), Sense::click(), 0.0);
        let rect = canvas.rect;
        let p = &canvas.painter;
        let pins = board.cpu.pins();
        let font = egui::FontId::proportional;
        self.regions.clear();

        p.rect_filled(rect, 8.0, Color32::from_rgb(170, 30, 35));
        let front = rect.shrink(10.0);

        // Schächte mit Flaschen und Wahltasten
        let shaft_w = 54.0;
        for i in 0..4 {
            let x = front.left() + 10.0 + i as f32 * (shaft_w + 10.0);
            let shaft = Rect::from_min_size(pos2(x, front.top() + 10.0), vec2(shaft_w, 200.0));
            p.rect_filled(shaft, 4.0, Color32::from_rgb(225, 235, 240));
            let (name, price, col) = PRODUCTS[i];
            for b in 0..self.stock[i] {
                let y = shaft.bottom() - 8.0 - b as f32 * 30.0;
                let bottle = Rect::from_center_size(
                    pos2(shaft.center().x - 3.0, y - 12.0),
                    vec2(36.0, 24.0),
                );
                p.rect_filled(bottle, 6.0, col);
                let neck = Rect::from_center_size(
                    pos2(bottle.right() + 4.0, bottle.center().y),
                    vec2(8.0, 8.0),
                );
                p.rect_filled(neck, 2.0, col);
            }
            if self.vending[i].is_some() {
                p.text(
                    shaft.center_top() + vec2(0.0, 10.0),
                    egui::Align2::CENTER_CENTER,
                    "Motor",
                    font(11.0),
                    Color32::RED,
                );
            }
            let btn_c = pos2(shaft.center().x, shaft.bottom() + 26.0);
            let fill = if self.stock[i] == 0 {
                Color32::from_rgb(230, 30, 30)
            } else if self.pressed[i] {
                Color32::from_rgb(60, 220, 80)
            } else {
                Color32::from_gray(210)
            };
            p.circle_filled(btn_c, 14.0, fill);
            p.circle_stroke(btn_c, 14.0, Stroke::new(1.5, Color32::BLACK));
            p.text(
                btn_c + vec2(0.0, 24.0),
                egui::Align2::CENTER_CENTER,
                name,
                font(12.0),
                Color32::WHITE,
            );
            p.text(
                btn_c + vec2(0.0, 38.0),
                egui::Align2::CENTER_CENTER,
                price,
                font(11.0),
                Color32::from_gray(230),
            );
            self.regions.push((
                Rect::from_center_size(btn_c, vec2(30.0, 30.0)),
                Click::Product(i),
            ));
        }

        // Rechte Spalte: Münzschlitz, Kreditanzeige, Geldrückgabe
        let col_x = front.right() - 72.0;
        let slot = Rect::from_min_size(pos2(col_x + 20.0, front.top() + 20.0), vec2(10.0, 44.0));
        // Für das Ablegen der Münzen: Schlitz in Bildschirmkoordinaten
        self.slot = Rect::from_min_max(
            rect.min + (slot.min - rect.min) * canvas.k,
            rect.min + (slot.max - rect.min) * canvas.k,
        );
        let accept = !self.tray_busy();
        p.rect_filled(slot.expand(6.0), 4.0, Color32::from_gray(40));
        p.rect_filled(
            slot,
            2.0,
            if accept {
                Color32::BLACK
            } else {
                Color32::from_rgb(120, 0, 0)
            },
        );
        p.text(
            slot.center_bottom() + vec2(0.0, 14.0),
            egui::Align2::CENTER_CENTER,
            "Münzen",
            font(11.0),
            Color32::WHITE,
        );
        if !self.falling.is_empty() {
            p.circle_filled(slot.center(), 6.0, Color32::from_rgb(210, 175, 60));
        }
        for (k, label) in ["0,50", "1,00", "2,00"].iter().enumerate() {
            let c = pos2(col_x + 25.0, front.top() + 110.0 + k as f32 * 22.0);
            let on = high(&pins, self.pins.kredit[k]);
            crate::periph::draw_led(
                p,
                c - vec2(16.0, 0.0),
                6.0,
                Color32::from_rgb(255, 210, 20),
                on,
            );
            p.text(
                c + vec2(4.0, 0.0),
                egui::Align2::LEFT_CENTER,
                *label,
                font(11.0),
                Color32::WHITE,
            );
        }
        let ab = Rect::from_min_size(pos2(col_x, front.top() + 180.0), vec2(64.0, 30.0));
        let ab_fill = if self.abbruch_t > 0.0 {
            Color32::from_rgb(255, 220, 60)
        } else {
            Color32::from_gray(210)
        };
        p.rect_filled(ab, 5.0, ab_fill);
        p.text(
            ab.center(),
            egui::Align2::CENTER_CENTER,
            "Rückgabe",
            font(11.0),
            Color32::BLACK,
        );
        self.regions.push((ab, Click::Abbruch));

        // Zwischenkasse mit Klappen
        let zk = Rect::from_min_size(pos2(col_x - 4.0, front.top() + 230.0), vec2(72.0, 50.0));
        p.rect_filled(zk, 4.0, Color32::from_gray(60));
        p.text(
            zk.center_top() + vec2(0.0, 9.0),
            egui::Align2::CENTER_CENTER,
            "Zwischenkasse",
            font(9.0),
            Color32::WHITE,
        );
        p.text(
            zk.center() + vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            euro(sum(&self.zwischenkasse)),
            font(13.0),
            Color32::WHITE,
        );
        let flap = |open: bool| {
            if open {
                Color32::from_rgb(60, 220, 80)
            } else {
                Color32::from_gray(150)
            }
        };
        let to_kasse = flap(high(&pins, self.pins.kasse[0]));
        let to_back = flap(high(&pins, self.pins.kasse[1]));
        p.text(
            zk.left_bottom() + vec2(0.0, 12.0),
            egui::Align2::LEFT_CENTER,
            "Kasse",
            font(10.0),
            to_kasse,
        );
        p.text(
            zk.right_bottom() + vec2(0.0, 12.0),
            egui::Align2::RIGHT_CENTER,
            "Rückgabe",
            font(10.0),
            to_back,
        );

        // Ausgabefach
        let tray = Rect::from_min_size(
            pos2(front.left() + 10.0, front.bottom() - 110.0),
            vec2(front.width() - 20.0, 70.0),
        );
        p.rect_filled(tray, 6.0, Color32::from_gray(25));
        p.text(
            tray.left_top() + vec2(8.0, 10.0),
            egui::Align2::LEFT_CENTER,
            "Ausgabefach",
            font(10.0),
            Color32::GRAY,
        );
        if let Some(i) = self.bottle_out {
            let b = Rect::from_center_size(tray.center() - vec2(60.0, -4.0), vec2(60.0, 26.0));
            p.rect_filled(b, 7.0, PRODUCTS[i].2);
            p.text(
                b.center(),
                egui::Align2::CENTER_CENTER,
                PRODUCTS[i].0,
                font(11.0),
                Color32::WHITE,
            );
        }
        for (k, &c) in self.rueckgabe.iter().enumerate().take(12) {
            let at = pos2(
                tray.center().x + 30.0 + k as f32 * 12.0,
                tray.center().y + 4.0,
            );
            p.circle_filled(at, COINS[c].2 * 0.7, Color32::from_rgb(210, 175, 60));
        }
        let klappe = Rect::from_min_size(
            pos2(tray.left(), tray.bottom() + 6.0),
            vec2(tray.width(), 24.0),
        );
        p.rect_filled(klappe, 4.0, Color32::from_gray(200));
        p.text(
            klappe.center(),
            egui::Align2::CENTER_CENTER,
            "Klappe (Ware und Geld entnehmen)",
            font(12.0),
            Color32::BLACK,
        );
        self.regions.push((klappe, Click::Klappe));

        let click = canvas.click_pos();
        canvas.finish();
        if let Some(pos) = click {
            let shift = ui.input(|i| i.modifiers.shift);
            if let Some(&(_, what)) = self.regions.iter().find(|(r, _)| r.contains(pos)) {
                self.click(what, shift);
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        egui::Window::new("Getränkeautomat: Pins").order(egui::Order::Foreground)
            .open(&mut self.settings)
            .resizable(false)
            .show(ctx, |ui| {
                let p = &mut self.pins;
                ui.columns(2, |cols| {
                    egui::Grid::new("gr_in").show(&mut cols[0], |ui| {
                        ui.label(RichText::new("Eingänge des PIC").strong());
                        ui.end_row();
                        for (i, (name, _, _)) in COINS.iter().enumerate() {
                            ui.label(format!("Münze {name}"));
                            changed |= pin_combo(ui, ("gm", i), &mut p.muenze[i]);
                            ui.end_row();
                        }
                        for i in 0..4 {
                            ui.label(format!("Taste {}", PRODUCTS[i].0));
                            changed |= pin_combo(ui, ("gt", i), &mut p.taster[i]);
                            ui.end_row();
                        }
                        for i in 0..4 {
                            ui.label(format!("Leer {}", PRODUCTS[i].0));
                            changed |= pin_combo(ui, ("gs", i), &mut p.sensor[i]);
                            ui.end_row();
                        }
                        ui.label("Geldrückgabe");
                        changed |= pin_combo(ui, "gab", &mut p.abbruch);
                        ui.end_row();
                    });
                    egui::Grid::new("gr_out").show(&mut cols[1], |ui| {
                        ui.label(RichText::new("Ausgänge des PIC").strong());
                        ui.end_row();
                        for i in 0..4 {
                            ui.label(format!("Motor {}", PRODUCTS[i].0));
                            changed |= pin_combo(ui, ("gmo", i), &mut p.motor[i]);
                            ui.end_row();
                        }
                        ui.label("Klappe zur Kasse");
                        changed |= pin_combo(ui, "gk1", &mut p.kasse[0]);
                        ui.end_row();
                        ui.label("Klappe zur Rückgabe");
                        changed |= pin_combo(ui, "gk2", &mut p.kasse[1]);
                        ui.end_row();
                        for (i, v) in ["0,50 €", "1,00 €", "2,00 €"].iter().enumerate() {
                            ui.label(format!("Anzeige {v}"));
                            changed |= pin_combo(ui, ("gkr", i), &mut p.kredit[i]);
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Bezahlung");
                    changed |= ui
                        .radio_value(&mut self.select_first, false, "Geld einwerfen, dann wählen")
                        .changed();
                    changed |= ui
                        .radio_value(&mut self.select_first, true, "Erst wählen, dann bezahlen")
                        .changed();
                });
            });
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Board, Getraenke) {
        let mut g = Getraenke::default();
        g.pins.muenze = [PinRef::port(1, 5), PinRef::port(1, 6), PinRef::port(1, 7)];
        g.pins.taster[0] = PinRef::port(1, 0);
        g.pins.motor[0] = PinRef::port(0, 0);
        g.pins.kasse = [PinRef::port(0, 1), PinRef::port(0, 2)];
        let mut b = Board::default();
        b.cpu.poke(0x85, 0x00); // Port A als Ausgang
        (b, g)
    }

    #[test]
    fn coin_pulse_then_zwischenkasse() {
        let (mut b, mut g) = setup();
        assert!(g.insert_coin(2));
        g.tick(&mut b, 0.05);
        assert!(b.cpu.pin(1, 7));
        g.tick(&mut b, 0.1);
        assert!(!b.cpu.pin(1, 7));
        assert_eq!(sum(&g.zwischenkasse), 200);
    }

    #[test]
    fn sale_flow() {
        let (mut b, mut g) = setup();
        g.insert_coin(2);
        g.tick(&mut b, 0.2);
        g.click(Click::Product(0), false);
        g.tick(&mut b, 0.01);
        assert!(b.cpu.pin(1, 0));
        // Programm leert die Zwischenkasse in die Kasse und startet Motor 1
        b.cpu.latch[0] = 0b011;
        g.tick(&mut b, 0.01);
        assert_eq!(sum(&g.kasse), 200);
        assert!(
            !b.cpu.pin(1, 0),
            "Wahltaste wird beim Motorstart zurückgesetzt"
        );
        for _ in 0..100 {
            g.tick(&mut b, 0.01);
        }
        assert_eq!(g.bottle_out, Some(0));
        assert_eq!(g.stock[0], FULL - 1);
        assert_eq!(
            b.cpu.latch[0] & 1,
            0,
            "Motor-Pin wird nach der Ausgabe zurückgesetzt"
        );
        // Ausgabefach belegt: keine Münzen mehr
        assert!(!g.insert_coin(0));
        g.click(Click::Klappe, false);
        assert!(g.insert_coin(0));
    }

    #[test]
    fn refill_with_shift() {
        let (_, mut g) = setup();
        g.stock[3] = 0;
        g.click(Click::Product(3), true);
        assert_eq!(g.stock[3], FULL);
        assert!(!g.pressed[3]);
    }
}
