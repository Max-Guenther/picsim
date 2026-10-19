//! Inhalte der Peripherie-Fenster: Ports, LCD, Siebensegment, Schalter,
//! Taster, LED-Array, Schrittmotor, Funktionsgenerator.
//!
//! Gezeichnete Bauteile wachsen mit der Fenstergröße (siehe `canvas`).

use crate::app::PicSimApp;
use crate::canvas::Canvas;
use eframe::egui::{self, pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use picsim_core::board::{LedLayout, KEY_NAMES};
use picsim_core::devices::{LcdColor, SegColor};
use picsim_core::pins::PORT_NAMES;
use picsim_core::{Cpu, SwitchPos};

fn font(size: f32) -> egui::FontId {
    egui::FontId::proportional(size)
}

// ----------------------------------------------------------------------
// Ports

/// Rechtsklick auf ein Bauteil öffnet seine Einstellungen.
fn settings_menu(resp: &egui::Response, open: &mut bool) {
    resp.context_menu(|ui| {
        if ui.button("Einstellungen…").clicked() {
            *open = true;
            ui.close();
        }
    });
}

pub fn ports(app: &mut PicSimApp, ui: &mut egui::Ui) {
    egui::ScrollArea::both()
        .id_salt("ports")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for port in 0..2usize {
                let width = Cpu::port_width(port);
                let tris = app.board.cpu.tris(port);
                let pins = app.board.cpu.pins()[port];
                egui::Grid::new(("port", port))
                    .spacing([5.0, 1.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new(PORT_NAMES[port]).strong());
                        for bit in (0..8).rev() {
                            ui.label(RichText::new(bit.to_string()).monospace().weak());
                        }
                        ui.end_row();
                        ui.label("Tris");
                        for bit in (0..8u8).rev() {
                            let t = if bit >= width {
                                "-"
                            } else if tris >> bit & 1 != 0 {
                                "i"
                            } else {
                                "o"
                            };
                            ui.label(RichText::new(t).monospace());
                        }
                        ui.end_row();
                        ui.label("Pin");
                        for bit in (0..8u8).rev() {
                            if bit >= width {
                                ui.label(RichText::new("-").monospace().weak());
                                continue;
                            }
                            let on = pins >> bit & 1 != 0;
                            let text = RichText::new(if on { "1" } else { "0" })
                                .monospace()
                                .strong();
                            let text = if on {
                                text.color(Color32::from_rgb(200, 40, 40))
                            } else {
                                text
                            };
                            let r = ui.add(egui::Label::new(text).sense(Sense::click()));
                            if r.clicked() {
                                app.board.cpu.toggle_pin(port as u8, bit);
                            }
                            r.on_hover_text(format!(
                                "{}{bit}: klicken zum Umschalten",
                                PORT_NAMES[port]
                            ));
                        }
                        ui.end_row();
                    });
                ui.add_space(4.0);
            }
        });
}

// ----------------------------------------------------------------------
// LCD

pub fn lcd(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let lcd = &app.board.lcd;
    let (rows, cols) = lcd.cfg.size();
    let (cw, ch) = (11.0, 18.0);
    let c = Canvas::new(
        ui,
        vec2(cols as f32 * cw + 24.0, rows as f32 * ch + 22.0),
        Sense::click(),
        0.0,
    );
    let p = &c.painter;
    p.rect_filled(c.rect, 4.0, Color32::from_rgb(0, 110, 40));
    let screen = c.rect.shrink2(vec2(10.0, 9.0));
    let (bg, cell, fg) = match lcd.cfg.color {
        LcdColor::Grau => (
            Color32::from_rgb(175, 190, 170),
            Color32::from_rgb(165, 180, 160),
            Color32::from_rgb(30, 35, 30),
        ),
        LcdColor::Gelb => (
            Color32::from_rgb(180, 205, 40),
            Color32::from_rgb(168, 192, 34),
            Color32::from_rgb(25, 35, 10),
        ),
        LcdColor::Blau => (
            Color32::from_rgb(40, 70, 200),
            Color32::from_rgb(48, 80, 210),
            Color32::from_rgb(235, 240, 255),
        ),
    };
    p.rect_filled(screen, 2.0, bg);
    let blink_phase = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
    for r in 0..rows {
        for col in 0..cols {
            let cr = Rect::from_min_size(
                screen.min + vec2(col as f32 * cw + 1.0, r as f32 * ch + 1.0),
                vec2(cw - 2.0, ch - 2.0),
            );
            p.rect_filled(cr, 0.0, cell);
            let Some(code) = lcd.char_at(r, col) else {
                continue;
            };
            if code < 16 {
                let pattern = lcd.custom_char(code);
                let (dx, dy) = (cr.width() / 5.0, cr.height() / 8.0);
                for (y, row) in pattern.iter().enumerate() {
                    for x in 0..5 {
                        if row >> (4 - x) & 1 != 0 {
                            let d = Rect::from_min_size(
                                cr.min + vec2(x as f32 * dx, y as f32 * dy),
                                vec2(dx - 0.3, dy - 0.3),
                            );
                            p.rect_filled(d, 0.0, fg);
                        }
                    }
                }
            } else {
                p.text(
                    cr.center(),
                    egui::Align2::CENTER_CENTER,
                    lcd_char(code),
                    egui::FontId::monospace(14.0),
                    fg,
                );
            }
            if lcd.cursor_at(r, col) {
                if lcd.cursor_on {
                    p.line_segment(
                        [
                            cr.left_bottom() + vec2(0.0, -1.5),
                            cr.right_bottom() + vec2(0.0, -1.5),
                        ],
                        Stroke::new(1.5, fg),
                    );
                }
                if lcd.blink_on && blink_phase {
                    p.rect_filled(cr, 0.0, fg.gamma_multiply(0.8));
                }
            }
        }
    }
    if lcd.blink_on && lcd.display_on {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    let hover = format!(
        "{rows} x {cols}, E={} RS={} RW={} D4..D7={} {} {} {}",
        lcd.cfg.e, lcd.cfg.rs, lcd.cfg.rw, lcd.cfg.d[4], lcd.cfg.d[5], lcd.cfg.d[6], lcd.cfg.d[7]
    );
    let resp = c.finish().on_hover_text(format!("{hover}\nRechtsklick: Einstellungen"));
    settings_menu(&resp, &mut app.dialogs.lcd);
}

/// Zeichensatz A00 des HD44780, soweit er von ASCII abweicht.
fn lcd_char(code: u8) -> String {
    match code {
        0x5C => "¥".into(),
        0x7E => "→".into(),
        0x7F => "←".into(),
        0xDF => "°".into(),
        0xE4 => "µ".into(),
        0xE1 => "ä".into(),
        0xEF => "ö".into(),
        0xF5 => "ü".into(),
        0xE2 => "ß".into(),
        0xF4 => "Ω".into(),
        0xFF => "█".into(),
        0x20..=0x7D => (code as char).to_string(),
        _ => "▯".into(),
    }
}

// ----------------------------------------------------------------------
// Siebensegment

fn seg_color(c: SegColor) -> Color32 {
    match c {
        SegColor::Rot => Color32::from_rgb(255, 40, 30),
        SegColor::Gruen => Color32::from_rgb(40, 255, 70),
        SegColor::Blau => Color32::from_rgb(60, 120, 255),
        SegColor::Gelb => Color32::from_rgb(255, 220, 30),
    }
}

pub fn seven_seg(app: &mut PicSimApp, ui: &mut egui::Ui) {
    // Leuchtdauer seit dem letzten Bild auswerten
    let pins = app.board.cpu.pins();
    app.board.sseg.take_frame(&pins, &mut app.seg_frame);

    let on = seg_color(app.board.sseg.cfg.color);
    let off = Color32::from_rgb(45, 12, 10).lerp_to_gamma(on, 0.08);
    let (dw, dh) = (30.0, 52.0);
    let c = Canvas::new(
        ui,
        vec2(8.0 * (dw + 6.0) + 10.0, dh + 14.0),
        Sense::click(),
        0.0,
    );
    c.painter
        .rect_filled(c.rect, 3.0, Color32::from_rgb(20, 8, 8));
    for d in 0..8 {
        // Stelle 1 rechts, wie üblich bei Multiplexanzeigen
        let x = c.rect.right() - 8.0 - (d as f32 + 1.0) * (dw + 6.0);
        let origin = pos2(x, c.rect.top() + 7.0);
        for s in 0..8 {
            draw_segment(
                &c.painter,
                origin,
                dw,
                dh,
                s,
                off.lerp_to_gamma(on, app.seg_frame[d][s]),
            );
        }
    }
    let resp = c.finish().on_hover_text("Rechtsklick: Einstellungen (Pins, Latch, Farbe)");
    settings_menu(&resp, &mut app.dialogs.sseg);
}

/// Segment s (a..g, dp) einer Stelle zeichnen.
fn draw_segment(p: &egui::Painter, o: Pos2, w: f32, h: f32, s: usize, col: Color32) {
    let t = 4.0;
    let slant = 3.0;
    let half = h / 2.0;
    let sk = |x: f32, y: f32| pos2(o.x + x + slant * (1.0 - y / h), o.y + y);
    let horiz = |y: f32| {
        vec![
            sk(t, y),
            sk(t + 2.0, y - t / 2.0),
            sk(w - t - 2.0, y - t / 2.0),
            sk(w - t, y),
            sk(w - t - 2.0, y + t / 2.0),
            sk(t + 2.0, y + t / 2.0),
        ]
    };
    let vert = |x: f32, y0: f32, y1: f32| {
        vec![
            sk(x, y0 + 1.0),
            sk(x + t / 2.0, y0 + t / 2.0 + 1.0),
            sk(x + t / 2.0, y1 - t / 2.0 - 1.0),
            sk(x, y1 - 1.0),
            sk(x - t / 2.0, y1 - t / 2.0 - 1.0),
            sk(x - t / 2.0, y0 + t / 2.0 + 1.0),
        ]
    };
    let pts = match s {
        0 => horiz(t / 2.0),
        1 => vert(w - t, t / 2.0, half),
        2 => vert(w - t, half, h - t / 2.0),
        3 => horiz(h - t / 2.0),
        4 => vert(t, half, h - t / 2.0),
        5 => vert(t, t / 2.0, half),
        6 => horiz(half),
        _ => {
            p.circle_filled(pos2(o.x + w + 1.0, o.y + h - 2.0), 2.2, col);
            return;
        }
    };
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

// ----------------------------------------------------------------------
// Schalter und Taster

/// Lage eines Schiebeschalters in Normalkoordinaten (oben 15..8, unten 7..0).
fn switch_rect(origin: Pos2, i: usize) -> Rect {
    let row = if i >= 8 { 0.0 } else { 1.0 };
    let col = 7.0 - (i % 8) as f32;
    Rect::from_min_size(
        origin + vec2(26.0 + col * 22.0, 14.0 + row * 72.0),
        vec2(14.0, 46.0),
    )
}

pub fn switches(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let c = Canvas::new(ui, vec2(206.0, 142.0), Sense::click(), 0.0);
    let o = c.rect.min;
    let p = &c.painter;
    for row in 0..2 {
        let y = o.y + 14.0 + row as f32 * 72.0;
        for (k, t) in ["H", "nc", "L"].iter().enumerate() {
            p.text(
                pos2(o.x + 2.0, y + 7.0 + k as f32 * 15.3),
                egui::Align2::LEFT_CENTER,
                *t,
                font(10.0),
                Color32::GRAY,
            );
        }
    }
    for i in 0..16 {
        let r = switch_rect(o, i);
        p.text(
            r.center_top() - vec2(0.0, 7.0),
            egui::Align2::CENTER_CENTER,
            i.to_string(),
            font(9.0),
            Color32::GRAY,
        );
        let body = if app.board.switches[i].is_connected() {
            Color32::from_rgb(190, 30, 30)
        } else {
            Color32::from_rgb(150, 110, 110)
        };
        p.rect_filled(r, 2.0, body);
        let third = r.height() / 3.0;
        let knob_y = match app.board.switch_pos[i] {
            SwitchPos::High => r.top(),
            SwitchPos::Open => r.top() + third,
            SwitchPos::Low => r.top() + 2.0 * third,
        };
        let knob = Rect::from_min_size(
            pos2(r.left() + 2.0, knob_y + 2.0),
            vec2(r.width() - 4.0, third - 4.0),
        );
        p.rect_filled(knob, 1.0, Color32::WHITE);
    }
    let hovered = c.resp.hover_pos().map(|pt| c.to_natural(pt));
    let click = c.click_pos();
    let resp = c.finish();
    if let Some(i) = hovered.and_then(|pt| (0..16).find(|&i| switch_rect(o, i).contains(pt))) {
        let name = &app.board.switch_names[i];
        let pin = app.board.switches[i];
        let text = if name.is_empty() {
            format!("S{i}: Pin {pin}")
        } else {
            format!("S{i} {name}: Pin {pin}")
        };
        resp.clone().on_hover_text(format!("{text}\nRechtsklick: Einstellungen"));
    }
    settings_menu(&resp, &mut app.dialogs.switches);
    if let Some(pt) = click {
        if let Some(i) = (0..16).find(|&i| switch_rect(o, i).contains(pt)) {
            let r = switch_rect(o, i);
            let third = r.height() / 3.0;
            let y = pt.y - r.top();
            let pos = if y < third {
                SwitchPos::High
            } else if y < 2.0 * third {
                SwitchPos::Open
            } else {
                SwitchPos::Low
            };
            app.board.set_switch(i, pos);
        }
    }
}

fn key_rect(origin: Pos2, i: usize) -> Rect {
    Rect::from_min_size(origin + vec2(4.0, 4.0 + i as f32 * 34.0), vec2(56.0, 28.0))
}

pub fn keys(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let c = Canvas::new(ui, vec2(64.0, 106.0), Sense::click(), 0.0);
    let o = c.rect.min;
    for i in 0..3 {
        let r = key_rect(o, i);
        let pressed = app.board.key_pressed[i];
        let fill = if pressed {
            Color32::from_rgb(170, 200, 250)
        } else {
            Color32::from_gray(225)
        };
        c.painter.rect_filled(r, 4.0, fill);
        c.painter.rect_stroke(
            r,
            4.0,
            Stroke::new(1.0, Color32::from_gray(120)),
            egui::StrokeKind::Inside,
        );
        let (size, col) = if pressed {
            (13.0, Color32::BLACK)
        } else {
            (12.0, Color32::from_gray(60))
        };
        c.painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            KEY_NAMES[i],
            font(size),
            col,
        );
    }
    let pointer = c.resp.interact_pointer_pos().map(|pt| c.to_natural(pt));
    let down = c.resp.is_pointer_button_down_on();
    let click = c.click_pos();
    let resp = c.finish().on_hover_text(
        "Rastende Taster: klicken. Nicht rastende: gedrückt halten\nRechtsklick: Einstellungen",
    );
    settings_menu(&resp, &mut app.dialogs.keys);
    for i in 0..3 {
        if app.board.key_latching[i] {
            if click.is_some_and(|pt| key_rect(o, i).contains(pt)) {
                let pressed = app.board.key_pressed[i];
                app.board.press_key(i, !pressed);
            }
        } else {
            let held = down && pointer.is_some_and(|pt| key_rect(o, i).contains(pt));
            if held != app.board.key_pressed[i] {
                app.board.press_key(i, held);
            }
        }
    }
}

// ----------------------------------------------------------------------
// LED-Array, Schrittmotor, Funktionsgenerator

pub fn led_color(i: usize) -> Color32 {
    match i {
        1 => Color32::from_rgb(40, 230, 60),
        2 => Color32::from_rgb(255, 210, 20),
        3 => Color32::from_rgb(50, 110, 255),
        4 => Color32::from_rgb(250, 250, 240),
        _ => Color32::from_rgb(255, 40, 30),
    }
}

/// Eine LED zeichnen, an oder aus.
pub fn draw_led(p: &egui::Painter, center: Pos2, r: f32, col: Color32, on: bool) {
    let fill = if on {
        col
    } else {
        col.lerp_to_gamma(Color32::from_rgb(60, 30, 30), 0.75)
    };
    if on {
        p.circle_filled(center, r + 4.0, col.gamma_multiply(0.25));
    }
    p.circle_filled(center, r, fill);
    p.circle_stroke(center, r, Stroke::new(1.0, Color32::from_gray(60)));
}

pub fn leds(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let gap = 30.0;
    let (cols, rows) = match app.board.led_layout {
        LedLayout::Wuerfel => (3, 3),
        LedLayout::Leuchtband => (9, 1),
    };
    let c = Canvas::new(
        ui,
        vec2(cols as f32 * gap, rows as f32 * gap),
        Sense::click(),
        0.0,
    );
    let mut hits = Vec::new();
    for i in 0..9 {
        let (col, row) = match app.board.led_layout {
            LedLayout::Wuerfel => (i % 3, i / 3),
            // Leuchtband: LED0 rechts wie Bit 0
            LedLayout::Leuchtband => (8 - i, 0),
        };
        let center = c.rect.min + vec2(col as f32 * gap + gap / 2.0, row as f32 * gap + gap / 2.0);
        draw_led(
            &c.painter,
            center,
            11.0,
            led_color(app.board.led_colors[i]),
            app.board.led_on(i),
        );
        hits.push(Rect::from_center_size(center, vec2(gap, gap)));
    }
    let hover = c.resp.hover_pos().map(|pt| c.to_natural(pt));
    let resp = c.finish();
    if let Some(i) = hover.and_then(|pt| hits.iter().position(|r| r.contains(pt))) {
        resp.clone()
            .on_hover_text(format!("LED{i}: {}\nRechtsklick: Einstellungen", app.board.leds[i]));
    }
    settings_menu(&resp, &mut app.dialogs.leds);
}

pub fn stepper(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let c = Canvas::new(ui, vec2(90.0, 104.0), Sense::click(), 0.0);
    let p = &c.painter;
    let center = c.rect.min + vec2(45.0, 45.0);
    p.circle_stroke(
        center,
        34.0,
        Stroke::new(6.0, Color32::from_rgb(30, 30, 40)),
    );
    let pins = app.board.cpu.pins();
    for (i, pin) in app.board.stepper.pins.iter().enumerate() {
        let active = picsim_core::devices::level(&pins, *pin) == Some(true);
        let a = (i as f32 * 90.0 - 90.0).to_radians();
        let at = center + vec2(a.cos(), a.sin()) * 34.0;
        let col = if active {
            Color32::from_rgb(230, 30, 30)
        } else {
            Color32::from_rgb(40, 60, 200)
        };
        p.rect_filled(Rect::from_center_size(at, vec2(12.0, 12.0)), 2.0, col);
    }
    let a = (app.board.stepper.angle - 90.0).to_radians();
    let dir = vec2(a.cos(), a.sin());
    p.line_segment(
        [center - dir * 20.0, center + dir * 24.0],
        Stroke::new(5.0, Color32::from_rgb(200, 170, 40)),
    );
    p.circle_filled(center, 7.0, Color32::from_rgb(160, 140, 40));
    let steps = format!("{} Halbschritte", app.board.stepper.steps);
    p.text(
        c.rect.center_bottom() - vec2(0.0, 6.0),
        egui::Align2::CENTER_CENTER,
        steps,
        font(10.0),
        Color32::GRAY,
    );
    let resp = c.finish().on_hover_text("Rechtsklick: Zähler zurücksetzen, Einstellungen");
    resp.context_menu(|ui| {
        if ui.button("Zähler zurücksetzen").clicked() {
            app.board.stepper.steps = 0;
            ui.close();
        }
        if ui.button("Einstellungen…").clicked() {
            app.dialogs.stepper = true;
            ui.close();
        }
    });
}

pub fn funcgen(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let g = &app.board.fgen;
    egui::Grid::new("fgen").show(ui, |ui| {
        ui.label("Pin");
        ui.label(RichText::new(g.pin.to_string()).monospace());
        ui.end_row();
        ui.label("Frequenz");
        ui.label(RichText::new(format!("{:.3} kHz", g.freq_khz)).monospace());
        ui.end_row();
        ui.label("Tastverhältnis");
        ui.label(RichText::new(format!("{}:{}", g.high, g.low)).monospace());
        ui.end_row();
    });
    if g.pin.is_connected() {
        let lvl = g.level_at(app.board.cpu.runtime_us());
        ui.label(RichText::new(if lvl { "Ausgang: H" } else { "Ausgang: L" }).small());
    }
    if ui.small_button("Einstellungen…").clicked() {
        app.dialogs.fgen = true;
    }
}
