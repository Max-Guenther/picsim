//! Datenspeicher, Spezialfunktionsregister und Listing.

use crate::app::PicSimApp;
use eframe::egui::{self, Color32, RichText, Sense};
use picsim_core::cpu::{INTCON, OPTION, STATUS};

fn changed(ui: &egui::Ui) -> Color32 {
    crate::theme::pick(ui, Color32::from_rgb(200, 30, 30), Color32::from_rgb(255, 100, 90))
}

fn current_line(ui: &egui::Ui) -> Color32 {
    crate::theme::pick(ui, Color32::from_rgb(150, 220, 160), Color32::from_rgb(40, 95, 55))
}
const BREAKPOINT: Color32 = Color32::from_rgb(210, 40, 40);

/// Fenster "Datenspeicher".
pub fn ram(app: &mut PicSimApp, ui: &mut egui::Ui) {
    egui::ScrollArea::both()
        .id_salt("ram")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("ramgrid")
                .spacing([6.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for c in 0..8 {
                        ui.label(RichText::new(format!("{c:02X}")).monospace().weak());
                    }
                    ui.end_row();
                    for row in 0..32u8 {
                        ui.label(RichText::new(format!("{:02X}", row * 8)).monospace().weak());
                        for col in 0..8u8 {
                            ram_cell(app, ui, row * 8 + col);
                        }
                        ui.end_row();
                    }
                });
        });
}

/// Ergebnis des Eingabefelds für eine Speicherzelle.
#[derive(Debug, PartialEq)]
enum CellEdit {
    /// Noch in Bearbeitung.
    Open,
    /// Mit Enter bestätigt (None: keine gültige Hexzahl).
    Commit(Option<u8>),
    /// Weggeklickt oder Escape.
    Cancel,
}

/// Eingabefeld für einen Hexwert. Holt sich beim ersten Bild den Fokus und
/// meldet, wenn die Eingabe beendet ist.
fn hex_edit(ui: &mut egui::Ui, text: &mut String) -> CellEdit {
    let r = ui.add(
        egui::TextEdit::singleline(text)
            .desired_width(20.0)
            .char_limit(2)
            .font(egui::TextStyle::Monospace),
    );
    // lost_focus erst prüfen: ein request_focus davor würde den Fokus
    // zurückholen, und Enter käme nie an
    if r.lost_focus() {
        return if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            CellEdit::Commit(u8::from_str_radix(text.trim(), 16).ok())
        } else {
            CellEdit::Cancel
        };
    }
    if !r.has_focus() {
        r.request_focus();
    }
    CellEdit::Open
}

fn ram_cell(app: &mut PicSimApp, ui: &mut egui::Ui, addr: u8) {
    if let Some((a, text)) = &mut app.ram_edit {
        if *a == addr {
            match hex_edit(ui, text) {
                CellEdit::Open => {}
                CellEdit::Commit(v) => {
                    if let Some(v) = v {
                        app.board.cpu.poke(addr, v);
                    }
                    app.ram_edit = None;
                }
                CellEdit::Cancel => app.ram_edit = None,
            }
            return;
        }
    }
    let v = app.board.cpu.peek(addr);
    let implemented = picsim_core::Cpu::map(addr).is_some();
    let mut t = RichText::new(format!("{v:02X}")).monospace();
    if !implemented {
        t = t.weak();
    } else if v != app.prev_ram[addr as usize] {
        t = t.color(changed(ui)).strong();
    }
    let r = ui.add(egui::Label::new(t).sense(Sense::click()));
    if r.clicked() && implemented {
        app.reg_edit = None;
        app.ram_edit = Some((addr, format!("{v:02X}")));
    }
    r.on_hover_text(format!(
        "Adresse {addr:02X}h: {v} dezimal, {v:08b}b. Klicken zum Ändern"
    ));
}

/// Acht anklickbare Bits eines Registers.
fn bit_row(app: &mut PicSimApp, ui: &mut egui::Ui, id: &str, addr: u8, names: [&str; 8]) {
    let v = app.board.cpu.peek(addr);
    egui::Grid::new(id).spacing([4.0, 0.0]).show(ui, |ui| {
        for n in names {
            ui.label(RichText::new(n).small().weak());
        }
        ui.end_row();
        for i in 0..8u8 {
            let bit = 7 - i;
            let on = v >> bit & 1 != 0;
            let r = ui.add(
                egui::Label::new(
                    RichText::new(if on { "1" } else { "0" })
                        .monospace()
                        .strong(),
                )
                .sense(Sense::click()),
            );
            if r.clicked() {
                app.board.cpu.poke(addr, v ^ (1 << bit));
            }
            r.on_hover_text("Klicken zum Umschalten");
        }
        ui.end_row();
    });
}

fn reg_grid(ui: &mut egui::Ui, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id).show(ui, |ui| {
        for (name, v) in rows {
            ui.label(*name);
            ui.label(RichText::new(v).monospace());
            ui.end_row();
        }
    });
}

/// Register, das sich in der Registeranzeige direkt ändern lässt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegField {
    /// Das W-Register liegt nicht im Datenspeicher.
    W,
    /// Register im Datenspeicher (FSR, PCL, PCLATH, Status).
    Addr(u8),
}

impl RegField {
    fn read(self, cpu: &picsim_core::Cpu) -> u8 {
        match self {
            RegField::W => cpu.w,
            RegField::Addr(STATUS) => cpu.status(),
            RegField::Addr(a) => cpu.peek(a),
        }
    }

    /// Schreibt wie ein Klick in den Datenspeicher (ohne Seiteneffekte eines Befehls).
    pub fn write(self, cpu: &mut picsim_core::Cpu, v: u8) {
        match self {
            RegField::W => cpu.w = v,
            RegField::Addr(a) => cpu.poke(a, v),
        }
    }
}

/// Name und Wert in einer Zeile, der Wert lässt sich anklicken und als
/// Hexzahl ändern (Enter übernimmt, Escape bricht ab).
fn reg_edit_row(app: &mut PicSimApp, ui: &mut egui::Ui, name: &str, field: RegField) {
    ui.label(name);
    if let Some((f, text)) = &mut app.reg_edit {
        if *f == field {
            match hex_edit(ui, text) {
                CellEdit::Open => {}
                CellEdit::Commit(v) => {
                    if let Some(v) = v {
                        field.write(&mut app.board.cpu, v);
                    }
                    app.reg_edit = None;
                }
                CellEdit::Cancel => app.reg_edit = None,
            }
            ui.end_row();
            return;
        }
    }
    let v = field.read(&app.board.cpu);
    let r = ui.add(egui::Label::new(RichText::new(format!("{v:02X}")).monospace()).sense(Sense::click()));
    if r.clicked() {
        app.ram_edit = None;
        app.reg_edit = Some((field, format!("{v:02X}")));
    }
    r.on_hover_text(format!("{name}: {v} dezimal, {v:08b}b. Klicken zum Ändern"));
    ui.end_row();
}

/// Fenster "Spezialfunktionsregister".
pub fn sfr(app: &mut PicSimApp, ui: &mut egui::Ui) {
    egui::ScrollArea::both()
        .id_salt("sfr")
        .auto_shrink([false, false])
        .show(ui, |ui| sfr_content(app, ui));
}

fn sfr_content(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let visible = [
        ("W-Reg", RegField::W),
        ("FSR", RegField::Addr(0x04)),
        ("PCL", RegField::Addr(0x02)),
        ("PCLATH", RegField::Addr(0x0A)),
        ("Status", RegField::Addr(STATUS)),
    ];
    let cpu = &app.board.cpu;
    let hidden = [
        ("PC", format!("{:04X}", cpu.pc)),
        ("Stackp.", format!("{}", cpu.sp)),
        ("Option", format!("{:02X}", cpu.peek(OPTION))),
        ("INTCON", format!("{:02X}", cpu.peek(INTCON))),
        ("WDT", format!("{:.1} ms", cpu.wdt_us / 1000.0)),
    ];
    ui.columns(2, |cols| {
        egui::Grid::new("sfr_visible").show(&mut cols[0], |ui| {
            for (name, field) in visible {
                reg_edit_row(app, ui, name, field);
            }
        });
        reg_grid(&mut cols[1], "sfr_hidden", &hidden);
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.board.cpu.wdt_enabled, "WDT aktiv");
        if app.board.cpu.sleeping {
            ui.label(RichText::new("SLEEP").color(crate::theme::pick(ui, Color32::DARK_BLUE, Color32::LIGHT_BLUE)).strong());
        }
    });
    ui.add_space(4.0);
    ui.label("Status");
    bit_row(
        app,
        ui,
        "status_bits",
        STATUS,
        ["IRP", "RP1", "RP0", "TO", "PD", "Z", "DC", "C"],
    );
    ui.label("Option");
    bit_row(
        app,
        ui,
        "option_bits",
        OPTION,
        ["RBP", "IntEdg", "T0CS", "T0SE", "PSA", "PS2", "PS1", "PS0"],
    );
    ui.label("INTCON");
    bit_row(
        app,
        ui,
        "intcon_bits",
        INTCON,
        [
            "GIE", "EEIE", "T0IE", "INTE", "RBIE", "T0IF", "INTF", "RBIF",
        ],
    );

    ui.add_space(4.0);
    ui.label("Stack");
    let cpu = &app.board.cpu;
    egui::Grid::new("stack").spacing([8.0, 0.0]).show(ui, |ui| {
        for i in 0..8 {
            let marker = if i as u8 == cpu.sp { "▶" } else { " " };
            ui.label(RichText::new(format!("{marker}{i}")).monospace().weak());
            ui.label(RichText::new(format!("{:04X}", cpu.stack[i])).monospace());
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    ui.add_space(4.0);
    ui.label("EEPROM");
    let ee = app.board.cpu.eeprom;
    egui::Grid::new("eeprom")
        .spacing([4.0, 0.0])
        .show(ui, |ui| {
            for row in 0..8 {
                ui.label(RichText::new(format!("{:02X}", row * 8)).monospace().weak());
                for c in 0..8 {
                    ui.label(RichText::new(format!("{:02X}", ee[row * 8 + c])).monospace());
                }
                ui.end_row();
            }
        });
}

pub fn listing(app: &mut PicSimApp, ui: &mut egui::Ui) {
    let current = app.board.listing.line_of(app.board.cpu.pc);
    let n = app.board.listing.lines.len();
    if n == 0 {
        ui.centered_and_justified(|ui| {
            ui.label("Keine LST-Datei geladen. Über Datei > LST-Datei öffnen laden oder im Editor einen Quelltext assemblieren.");
        });
        return;
    }
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let row_h = ui.ctx().fonts_mut(|f| f.row_height(&font)).max(16.0);
    ui.spacing_mut().item_spacing.y = 0.0;

    let mut area = egui::ScrollArea::both()
        .id_salt("listing")
        .auto_shrink([false, false]);
    if app.scroll_to_pc {
        if let Some(line) = current {
            let visible = ui.available_height();
            area = area.vertical_scroll_offset((line as f32 * row_h - visible / 2.0).max(0.0));
        }
        app.scroll_to_pc = false;
    }
    let mut toggle = None;
    let mut visible = 0..0;
    area.show_rows(ui, row_h, n, |ui, range| {
        visible = range.clone();
        for i in range {
            let line = &app.board.listing.lines[i];
            let width = ui.available_width().max(900.0);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, row_h), Sense::click());
            let painter = ui.painter_at(rect);
            if Some(i) == current {
                painter.rect_filled(rect, 0.0, current_line(ui));
            }
            let in_gutter = |p: Option<egui::Pos2>| p.is_some_and(|p| p.x < rect.left() + 20.0);
            let bp_center = rect.left_center() + egui::vec2(9.0, 0.0);
            if let Some((addr, _)) = line.code {
                if app.board.breakpoints.contains(&addr) {
                    painter.circle_filled(bp_center, row_h * 0.32, BREAKPOINT);
                } else if resp.hovered() && in_gutter(ui.input(|i| i.pointer.hover_pos())) {
                    painter.circle_stroke(bp_center, row_h * 0.32, (1.0, BREAKPOINT));
                }
                if resp.clicked() && in_gutter(ui.input(|i| i.pointer.interact_pos())) {
                    toggle = Some(addr);
                }
            }
            painter.text(
                rect.left_center() + egui::vec2(22.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &line.text,
                font.clone(),
                ui.visuals().text_color(),
            );
        }
    });
    if let Some(addr) = toggle {
        app.board.toggle_breakpoint(addr);
    }
    // Während Go mitscrollen, sobald die aktuelle Zeile aus dem Bild läuft
    if let Some(line) = current {
        let inner = visible.start + 1..visible.end.saturating_sub(1);
        if app.board.running() && !inner.contains(&line) {
            app.scroll_to_pc = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein Bild mit den gegebenen Tasten zeichnen.
    fn frame(ctx: &egui::Context, text: &mut String, keys: &[egui::Key]) -> CellEdit {
        let mut input = egui::RawInput::default();
        input.focused = true;
        for &key in keys {
            for pressed in [true, false] {
                input.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
            }
        }
        let mut res = CellEdit::Open;
        let mut out = ctx.run_ui(input, |ui| res = hex_edit(ui, text));
        out.textures_delta.clear();
        res
    }

    #[test]
    fn enter_commits_value() {
        let ctx = egui::Context::default();
        let mut text = "00".to_string();
        assert_eq!(frame(&ctx, &mut text, &[]), CellEdit::Open);
        assert_eq!(frame(&ctx, &mut text, &[]), CellEdit::Open);
        text = "3F".into();
        assert_eq!(frame(&ctx, &mut text, &[egui::Key::Enter]), CellEdit::Commit(Some(0x3F)));
    }

    #[test]
    fn w_register_is_writable() {
        let mut cpu = picsim_core::Cpu::new();
        RegField::W.write(&mut cpu, 0xA5);
        assert_eq!(cpu.w, 0xA5);
        assert_eq!(RegField::W.read(&cpu), 0xA5);
        // Nicht im Datenspeicher gelandet
        assert!((0..=255u8).all(|a| cpu.peek(a) != 0xA5));
        RegField::Addr(0x04).write(&mut cpu, 0x3C);
        assert_eq!(RegField::Addr(0x04).read(&cpu), 0x3C);
        assert_eq!(cpu.peek(0x84), 0x3C, "FSR ist in beiden Bänken gleich");
    }

    /// Klick auf den W-Wert, Hexzahl tippen, Enter: W hat den neuen Wert.
    #[test]
    fn w_register_edit_in_view() {
        let root = std::env::temp_dir().join(format!("picsim-wreg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let config = root.join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("picsimu.ini"), "[UPDATE]\nAUTO=0\n").unwrap();
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = PicSimApp::with_dirs(&cc, None, config, root.join("cache"));
        app.reg_edit = Some((RegField::W, "00".into()));
        let run = |app: &mut PicSimApp, keys: &[egui::Key]| {
            let mut input = egui::RawInput { focused: true, ..Default::default() };
            for &key in keys {
                for pressed in [true, false] {
                    input.events.push(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
                }
            }
            let mut out = ctx.run_ui(input, |ui| sfr_content(app, ui));
            out.textures_delta.clear();
        };
        run(&mut app, &[]);
        run(&mut app, &[]);
        app.reg_edit.as_mut().unwrap().1 = "7E".into();
        run(&mut app, &[egui::Key::Enter]);
        assert_eq!(app.board.cpu.w, 0x7E);
        assert!(app.reg_edit.is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn escape_cancels() {
        let ctx = egui::Context::default();
        let mut text = "00".to_string();
        frame(&ctx, &mut text, &[]);
        frame(&ctx, &mut text, &[]);
        assert_eq!(frame(&ctx, &mut text, &[egui::Key::Escape]), CellEdit::Cancel);
    }
}
