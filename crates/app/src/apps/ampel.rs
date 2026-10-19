//! Applikation "Ampeln an einer Kreuzung".
//!
//! Hauptstraße waagerecht (Auto 1 und 3), Nebenstraße senkrecht (Auto 2
//! und 4). Jedes Auto gehorcht der Ampel mit seiner Nummer. Steht ein Auto
//! auf der Kontaktschleife vor der Haltelinie, liegt am Kontakt-Pin High.

use crate::canvas::Canvas;
use crate::settings::pin_combo;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use picsim_core::devices::level;
use picsim_core::{Board, Ini, PinRef};

const SIZE: f32 = 420.0;
const ROAD: f32 = 30.0; // halbe Straßenbreite
const CAR_LEN: f32 = 26.0;
const CAR_W: f32 = 14.0;
const ENTRY: f32 = 30.0; // Start außerhalb des Bildes
const STOP_GAP: f32 = 34.0; // Haltelinie vor der Kreuzungsmitte
const CONTACT_LEN: f32 = 40.0;
/// So nah an der Haltelinie fährt ein Auto bei Rot noch durch.
const COMMIT: f32 = 8.0;
/// Pixel je Simulationssekunde bei Tempo 1.
const SPEED_UNIT: f32 = 15.0;

const CAR_COLORS: [Color32; 4] = [
    Color32::from_rgb(30, 90, 200),
    Color32::from_rgb(200, 60, 30),
    Color32::from_rgb(40, 150, 60),
    Color32::from_rgb(160, 60, 170),
];

#[derive(Clone, Copy, Default)]
pub struct Light {
    pub rot: PinRef,
    pub gelb: PinRef,
    pub gruen: PinRef,
    pub kontakt: PinRef,
}

#[derive(Clone, Copy)]
struct Car {
    enabled: bool,
    speed: f32,
    /// Weg der Fahrzeugfront ab dem Startpunkt.
    s: f32,
    held: bool,
}

pub struct Ampel {
    pub open: bool,
    pub settings: bool,
    pub lights: [Light; 4],
    cars: [Car; 4],
    crashed: bool,
}

impl Default for Ampel {
    fn default() -> Self {
        Ampel {
            open: false,
            settings: false,
            lights: [Light::default(); 4],
            cars: std::array::from_fn(|i| Car {
                enabled: false,
                speed: 3.0,
                s: i as f32 * 60.0,
                held: false,
            }),
            crashed: false,
        }
    }
}

fn center() -> Pos2 {
    pos2(SIZE / 2.0, SIZE / 2.0)
}

/// Startpunkt und Fahrtrichtung von Auto i.
fn path(i: usize) -> (Pos2, Vec2) {
    let c = center();
    let lane = ROAD / 2.0;
    match i {
        0 => (pos2(-ENTRY, c.y + lane), vec2(1.0, 0.0)),
        1 => (pos2(c.x - lane, -ENTRY), vec2(0.0, 1.0)),
        2 => (pos2(SIZE + ENTRY, c.y - lane), vec2(-1.0, 0.0)),
        _ => (pos2(c.x + lane, SIZE + ENTRY), vec2(0.0, -1.0)),
    }
}

fn s_stop() -> f32 {
    ENTRY + SIZE / 2.0 - STOP_GAP
}

fn s_max() -> f32 {
    SIZE + 2.0 * ENTRY + CAR_LEN
}

/// Rechteck eines Autos in Bildkoordinaten (ohne Versatz).
fn car_rect(i: usize, s: f32) -> Rect {
    let (o, d) = path(i);
    let front = o + d * s;
    let back = front - d * CAR_LEN;
    let side = vec2(d.y.abs(), d.x.abs()) * (CAR_W / 2.0);
    Rect::from_two_pos(front - side, back + side)
}

fn lamp(pins: &[u8; 5], p: PinRef) -> bool {
    level(pins, p) == Some(true)
}

impl Ampel {
    pub fn load(&mut self, ini: &Ini) {
        for i in 0..4 {
            let sec = format!("Ampel{}", i + 1);
            self.lights[i] = Light {
                rot: ini.pin(&sec, "Rot"),
                gelb: ini.pin(&sec, "Gelb"),
                gruen: ini.pin(&sec, "Gruen"),
                kontakt: ini.pin(&sec, "Kontakt"),
            };
            let car = &mut self.cars[i];
            car.enabled = ini.flag("Autos", &format!("Auto{}", i + 1));
            car.speed = ini
                .number("Autos", &format!("Tempo{}", i + 1))
                .unwrap_or(3.0)
                .clamp(1.0, 10.0) as f32;
        }
    }

    pub fn store(&self, ini: &mut Ini) {
        for (i, l) in self.lights.iter().enumerate() {
            let sec = format!("Ampel{}", i + 1);
            ini.set_pin(&sec, "Rot", l.rot);
            ini.set_pin(&sec, "Gelb", l.gelb);
            ini.set_pin(&sec, "Gruen", l.gruen);
            ini.set_pin(&sec, "Kontakt", l.kontakt);
            ini.set_flag("Autos", &format!("Auto{}", i + 1), self.cars[i].enabled);
            ini.set(
                "Autos",
                &format!("Tempo{}", i + 1),
                format!("{:.1}", self.cars[i].speed),
            );
        }
    }

    /// Muss das Auto vor dieser Ampel halten? Rot, Rot-Gelb und Gelb heißt halten.
    fn must_stop(&self, pins: &[u8; 5], i: usize) -> bool {
        let l = &self.lights[i];
        lamp(pins, l.rot) || (lamp(pins, l.gelb) && !lamp(pins, l.gruen))
    }

    /// Bewegt die Autos um `dt` Simulationssekunden und setzt die Kontakte.
    pub fn tick(&mut self, board: &mut Board, dt: f32) {
        let pins = board.cpu.pins();
        if !self.crashed {
            for i in 0..4 {
                if !self.cars[i].enabled {
                    continue;
                }
                let stop = self.must_stop(&pins, i);
                let car = &mut self.cars[i];
                let mut next = car.s + car.speed * SPEED_UNIT * dt;
                if stop && car.s <= s_stop() && (car.held || s_stop() - car.s > COMMIT) {
                    next = next.min(s_stop());
                    car.held = true;
                } else {
                    car.held = false;
                }
                car.s = if next > s_max() { 0.0 } else { next };
            }
            self.crashed = !self.collisions().is_empty();
        }
        for i in 0..4 {
            if let PinRef::Port { port, bit } = self.lights[i].kontakt {
                let car = &self.cars[i];
                let on =
                    car.enabled && car.s >= s_stop() - CONTACT_LEN && car.s - CAR_LEN <= s_stop();
                board.cpu.set_input(port, bit, on);
            }
        }
    }

    fn collisions(&self) -> Vec<(usize, usize)> {
        (0..4)
            .flat_map(|a| (a + 1..4).map(move |b| (a, b)))
            .filter(|&(a, b)| {
                self.cars[a].enabled
                    && self.cars[b].enabled
                    && car_rect(a, self.cars[a].s).intersects(car_rect(b, self.cars[b].s))
            })
            .collect()
    }

    /// Klick auf "Crash": beteiligte Autos ein Stück weiterschieben.
    fn resolve_crash(&mut self) {
        for (a, b) in self.collisions() {
            self.cars[a].s += 12.0;
            self.cars[b].s += 12.0;
        }
        self.crashed = !self.collisions().is_empty();
    }

    pub fn show(&mut self, ctx: &egui::Context, board: &Board) -> bool {
        let mut open = self.open;
        egui::Window::new("Ampeln an einer Kreuzung").order(egui::Order::Foreground)
            .open(&mut open)
            .resizable(true)
            .default_size([640.0, 440.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        for i in 0..4 {
                            let car = &mut self.cars[i];
                            ui.horizontal(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                ui.painter().rect_filled(r, 2.0, CAR_COLORS[i]);
                                ui.checkbox(&mut car.enabled, format!("Auto {}", i + 1));
                            });
                            ui.add(egui::Slider::new(&mut car.speed, 1.0..=10.0).text("Tempo"));
                            ui.add_space(4.0);
                        }
                        ui.label(
                            RichText::new("Auto 1 und 3: Hauptstraße\nAuto 2 und 4: Nebenstraße")
                                .small(),
                        );
                        if ui.button("Einstellungen…").clicked() {
                            self.settings = true;
                        }
                        if ui.button("Autos zurücksetzen").clicked() {
                            for (i, c) in self.cars.iter_mut().enumerate() {
                                c.s = i as f32 * 60.0;
                                c.held = false;
                            }
                            self.crashed = false;
                        }
                    });
                    self.draw(ui, board);
                });
            });
        self.open = open;

        let mut changed = false;
        egui::Window::new("Ampeln: Pins").order(egui::Order::Foreground)
            .open(&mut self.settings)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("ampel_pins").show(ui, |ui| {
                    for name in ["", "Rot", "Gelb", "Grün", "Kontakt"] {
                        ui.label(name);
                    }
                    ui.end_row();
                    for (i, l) in self.lights.iter_mut().enumerate() {
                        ui.label(format!("Ampel {}", i + 1));
                        changed |= pin_combo(ui, ("ar", i), &mut l.rot);
                        changed |= pin_combo(ui, ("ag", i), &mut l.gelb);
                        changed |= pin_combo(ui, ("agr", i), &mut l.gruen);
                        changed |= pin_combo(ui, ("ak", i), &mut l.kontakt);
                        ui.end_row();
                    }
                });
                ui.label(
                    RichText::new("Lampen sind Ausgänge des PIC, Kontakte sind Eingänge.").small(),
                );
            });
        changed
    }

    fn draw(&mut self, ui: &mut egui::Ui, board: &Board) {
        let canvas = Canvas::new(ui, vec2(SIZE, SIZE), Sense::click(), 0.0);
        let rect = canvas.rect;
        let p = &canvas.painter;
        let o = rect.min.to_vec2();
        let c = center() + o;
        p.rect_filled(rect, 0.0, Color32::from_rgb(110, 160, 90));
        let road = Color32::from_gray(70);
        p.rect_filled(
            Rect::from_min_max(
                pos2(rect.left(), c.y - ROAD),
                pos2(rect.right(), c.y + ROAD),
            ),
            0.0,
            road,
        );
        p.rect_filled(
            Rect::from_min_max(
                pos2(c.x - ROAD, rect.top()),
                pos2(c.x + ROAD, rect.bottom()),
            ),
            0.0,
            road,
        );
        // Mittellinien
        let dash = Stroke::new(1.5, Color32::from_gray(220));
        let mut t = 0.0;
        while t < SIZE {
            if (rect.left() + t - c.x).abs() > ROAD {
                p.line_segment(
                    [
                        pos2(rect.left() + t, c.y),
                        pos2(rect.left() + t + 10.0, c.y),
                    ],
                    dash,
                );
            }
            if (rect.top() + t - c.y).abs() > ROAD {
                p.line_segment(
                    [pos2(c.x, rect.top() + t), pos2(c.x, rect.top() + t + 10.0)],
                    dash,
                );
            }
            t += 20.0;
        }

        let pins = board.cpu.pins();
        for i in 0..4 {
            let (start, d) = path(i);
            let stop = start + d * s_stop() + o;
            let side = vec2(-d.y, d.x); // rechts in Fahrtrichtung
            let half = side * (ROAD / 2.0);
            p.line_segment([stop - half, stop + half], Stroke::new(3.0, Color32::WHITE));
            if self.lights[i].kontakt.is_connected() {
                let loop_rect =
                    Rect::from_two_pos(stop - d * CONTACT_LEN - half * 0.7, stop + half * 0.7);
                let col = if lamp(&pins, self.lights[i].kontakt) {
                    Color32::from_rgb(255, 200, 0)
                } else {
                    Color32::from_gray(140)
                };
                p.rect_stroke(
                    loop_rect,
                    0.0,
                    Stroke::new(1.5, col),
                    egui::StrokeKind::Inside,
                );
            }
            // Ampel rechts neben der Haltelinie
            let pos = stop + side * (ROAD / 2.0 + 14.0) - d * 6.0;
            let housing = Rect::from_center_size(pos, vec2(14.0, 38.0));
            p.rect_filled(housing, 3.0, Color32::from_gray(30));
            let l = self.lights[i];
            let lamps = [
                (l.rot, Color32::from_rgb(255, 30, 20)),
                (l.gelb, Color32::from_rgb(255, 200, 0)),
                (l.gruen, Color32::from_rgb(30, 230, 60)),
            ];
            for (k, (pin, col)) in lamps.into_iter().enumerate() {
                let fill = if lamp(&pins, pin) {
                    col
                } else {
                    col.gamma_multiply(0.18)
                };
                p.circle_filled(
                    pos2(pos.x, housing.top() + 7.0 + k as f32 * 12.0),
                    4.5,
                    fill,
                );
            }
            let font = egui::FontId::proportional(10.0);
            p.text(
                housing.center_bottom() + vec2(0.0, 8.0),
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                font,
                Color32::WHITE,
            );
        }

        for (i, car) in self.cars.iter().enumerate() {
            if car.enabled {
                let r = car_rect(i, car.s).translate(o);
                p.rect_filled(r, 3.0, CAR_COLORS[i]);
                p.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.0, Color32::BLACK),
                    egui::StrokeKind::Inside,
                );
            }
        }

        let label = Rect::from_center_size(c, vec2(110.0, 40.0));
        if self.crashed {
            p.rect_filled(label, 6.0, Color32::from_rgb(220, 20, 20));
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "Crash",
                egui::FontId::proportional(26.0),
                Color32::WHITE,
            );
        }
        let click = canvas.click_pos();
        let resp = canvas.finish();
        if self.crashed {
            if click.is_some_and(|pt| label.contains(pt)) {
                self.resolve_crash();
            }
            resp.on_hover_text("Crash anklicken, bis die Autos auseinander sind");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(red: bool) -> (Board, Ampel) {
        let mut a = Ampel::default();
        a.lights[0].rot = if red { PinRef::Vdd } else { PinRef::Gnd };
        a.lights[0].kontakt = PinRef::port(1, 7);
        a.cars[0].enabled = true;
        a.cars[0].s = 0.0;
        (Board::default(), a)
    }

    #[test]
    fn car_stops_at_red_and_sets_contact() {
        let (mut b, mut a) = setup(true);
        for _ in 0..200 {
            a.tick(&mut b, 0.1);
        }
        assert_eq!(a.cars[0].s, s_stop());
        assert!(b.cpu.pin(1, 7));
    }

    #[test]
    fn car_drives_through_on_green() {
        let (mut b, mut a) = setup(false);
        for _ in 0..200 {
            a.tick(&mut b, 0.1);
        }
        assert_ne!(a.cars[0].s, s_stop());
    }

    #[test]
    fn crossing_cars_crash() {
        let mut b = Board::default();
        let mut a = Ampel::default();
        a.cars[0] = Car {
            enabled: true,
            speed: 1.0,
            s: ENTRY + SIZE / 2.0 + 10.0,
            held: false,
        };
        a.cars[1] = Car {
            enabled: true,
            speed: 1.0,
            s: ENTRY + SIZE / 2.0 + 15.0,
            held: false,
        };
        a.tick(&mut b, 0.01);
        assert!(a.crashed);
        for _ in 0..10 {
            a.resolve_crash();
        }
        assert!(!a.crashed);
    }
}
