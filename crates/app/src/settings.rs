//! Einstellungsfenster für die Peripherie, dazu Hilfe und Info.

use eframe::egui::{self, RichText};
use picsim_core::board::{LedLayout, KEY_NAMES, LED_COLORS};
use picsim_core::devices::{LcdColor, SegColor, LCD_FORMATS};
use picsim_core::{Board, PinRef};

#[derive(Default)]
pub struct Dialogs {
    pub leds: bool,
    pub switches: bool,
    pub keys: bool,
    pub sseg: bool,
    pub lcd: bool,
    pub fgen: bool,
    pub stepper: bool,
    pub xtal: bool,
    pub help: bool,
    pub about: bool,
    xtal_text: String,
}

/// Alle Pins, die man anschließen kann.
fn pin_choices() -> Vec<PinRef> {
    let mut v = vec![PinRef::Nc, PinRef::Vdd, PinRef::Gnd];
    v.extend((0..5).map(|b| PinRef::port(0, b)));
    v.extend((0..8).map(|b| PinRef::port(1, b)));
    v
}

/// Auswahlliste für einen Pin. Liefert true bei Änderung.
pub fn pin_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, pin: &mut PinRef) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(64.0).selected_text(pin.to_string()).show_ui(ui, |ui| {
        for p in pin_choices() {
            changed |= ui.selectable_value(pin, p, p.to_string()).changed();
        }
    });
    changed
}

fn window<'a>(title: &'a str, open: &'a mut bool) -> egui::Window<'a> {
    egui::Window::new(title).open(open).collapsible(false).resizable(false)
}

impl Dialogs {
    pub fn menu(&mut self, ui: &mut egui::Ui) {
        let items: [(&str, &mut bool); 8] = [
            ("LED-Array…", &mut self.leds),
            ("Schiebeschalter…", &mut self.switches),
            ("Taster…", &mut self.keys),
            ("Siebensegmentanzeigen…", &mut self.sseg),
            ("LCD…", &mut self.lcd),
            ("Funktionsgenerator…", &mut self.fgen),
            ("Schrittmotor…", &mut self.stepper),
            ("Quarzfrequenz…", &mut self.xtal),
        ];
        for (name, flag) in items {
            if ui.button(name).clicked() {
                *flag = true;
            }
        }
    }

    /// Zeigt offene Einstellungsfenster. Liefert true, wenn sich etwas geändert hat.
    pub fn show(&mut self, ctx: &egui::Context, b: &mut Board) -> bool {
        let mut changed = false;

        window("LED-Array", &mut self.leds).show(ctx, |ui| {
            egui::Grid::new("led_cfg").show(ui, |ui| {
                ui.label("");
                ui.label("Pin");
                ui.label("Farbe");
                ui.end_row();
                for i in 0..9 {
                    ui.label(format!("LED{i}"));
                    changed |= pin_combo(ui, ("led", i), &mut b.leds[i]);
                    egui::ComboBox::from_id_salt(("ledcol", i))
                        .width(70.0)
                        .selected_text(LED_COLORS[b.led_colors[i]])
                        .show_ui(ui, |ui| {
                            for (c, name) in LED_COLORS.iter().enumerate() {
                                changed |= ui.selectable_value(&mut b.led_colors[i], c, *name).changed();
                            }
                        });
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Anordnung");
                changed |= ui.radio_value(&mut b.led_layout, LedLayout::Wuerfel, "Würfel (3x3)").changed();
                changed |= ui.radio_value(&mut b.led_layout, LedLayout::Leuchtband, "Leuchtband").changed();
            });
        });

        window("Schiebeschalter", &mut self.switches).show(ctx, |ui| {
            ui.label("Der Name erscheint als Hinweis, wenn die Maus über dem Schalter steht.");
            egui::Grid::new("sw_cfg").show(ui, |ui| {
                for i in 0..16 {
                    ui.label(format!("S{i}"));
                    if pin_combo(ui, ("sw", i), &mut b.switches[i]) {
                        changed = true;
                        b.drive_inputs();
                    }
                    let name = egui::TextEdit::singleline(&mut b.switch_names[i]).desired_width(110.0).hint_text("Name");
                    changed |= ui.add(name).changed();
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        });

        window("Taster", &mut self.keys).show(ctx, |ui| {
            egui::Grid::new("key_cfg").show(ui, |ui| {
                for (i, name) in KEY_NAMES.iter().enumerate() {
                    ui.label(*name);
                    if pin_combo(ui, ("key", i), &mut b.keys[i]) {
                        changed = true;
                        b.drive_inputs();
                    }
                    changed |= ui.checkbox(&mut b.key_latching[i], "rastend").changed();
                    ui.end_row();
                }
            });
            ui.label(RichText::new("Gedrückt liegt High am Pin, losgelassen Low.").small());
        });

        window("Siebensegmentanzeigen", &mut self.sseg).show(ctx, |ui| {
            let cfg = &mut b.sseg.cfg;
            ui.label("Multiplexbetrieb: alle Segmente sind parallel, Digitselect wählt die Stelle.");
            egui::Grid::new("seg_cfg").show(ui, |ui| {
                for (i, name) in ["a", "b", "c", "d", "e", "f", "g", "dp"].iter().enumerate() {
                    ui.label(format!("Segment {name}"));
                    changed |= pin_combo(ui, ("seg", i), &mut cfg.seg[i]);
                    ui.label(format!("Digit {}", i + 1));
                    changed |= pin_combo(ui, ("dig", i), &mut cfg.digit[i]);
                    changed |= ui.checkbox(&mut cfg.latch[i], "Latch").changed();
                    ui.end_row();
                }
            });
            ui.separator();
            changed |= ui.checkbox(&mut cfg.common_anode, "Gemeinsame Anode (Segmente aktiv bei Low)").changed();
            changed |= ui.checkbox(&mut cfg.digit_inverted, "Negiertes Digitselect (z.B. über Transistor)").changed();
            ui.horizontal(|ui| {
                ui.label("Farbe");
                for (c, n) in [(SegColor::Rot, "rot"), (SegColor::Gruen, "grün"), (SegColor::Blau, "blau"), (SegColor::Gelb, "gelb")] {
                    changed |= ui.radio_value(&mut cfg.color, c, n).changed();
                }
            });
        });

        window("LCD", &mut self.lcd).show(ctx, |ui| {
            let cfg = &mut b.lcd.cfg;
            ui.label("Die Anzeige arbeitet im 4-Bit-Modus an D4..D7 und braucht die übliche Initialisierung.");
            egui::Grid::new("lcd_cfg").show(ui, |ui| {
                for (name, pin) in [("E", &mut cfg.e), ("RS", &mut cfg.rs), ("RW", &mut cfg.rw)] {
                    ui.label(name);
                    changed |= pin_combo(ui, ("lcdpin", name), pin);
                    ui.end_row();
                }
                for i in 4..8 {
                    ui.label(format!("D{i}"));
                    changed |= pin_combo(ui, ("lcdd", i), &mut cfg.d[i]);
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Format");
                for (i, (r, c)) in LCD_FORMATS.iter().enumerate() {
                    changed |= ui.radio_value(&mut cfg.format, i, format!("{r} x {c}")).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Farbe");
                for (c, n) in [(LcdColor::Grau, "grau"), (LcdColor::Gelb, "gelb"), (LcdColor::Blau, "blau")] {
                    changed |= ui.radio_value(&mut cfg.color, c, n).changed();
                }
            });
        });

        window("Funktionsgenerator", &mut self.fgen).show(ctx, |ui| {
            let g = &mut b.fgen;
            ui.label("Rechtecksignal auf Kanal 1. Mit nc ist der Generator aus.");
            egui::Grid::new("fgen_cfg").show(ui, |ui| {
                ui.label("Pin");
                changed |= pin_combo(ui, "fgenpin", &mut g.pin);
                ui.end_row();
                ui.label("Frequenz (kHz)");
                let freq = egui::DragValue::new(&mut g.freq_khz).speed(0.01).range(0.0001..=1000.0).max_decimals(4);
                changed |= ui.add(freq).changed();
                ui.end_row();
                ui.label("Tastverhältnis H:L");
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut g.high).range(0.0..=10000.0)).changed();
                    ui.label(":");
                    changed |= ui.add(egui::DragValue::new(&mut g.low).range(0.0..=10000.0)).changed();
                });
                ui.end_row();
            });
            ui.label(RichText::new("50 Hz entspricht 0,05 kHz.").small());
        });

        window("Schrittmotor", &mut self.stepper).show(ctx, |ui| {
            egui::Grid::new("step_cfg").show(ui, |ui| {
                for i in 0..4 {
                    ui.label(format!("Spule {} ({})", i + 1, ["oben", "rechts", "unten", "links"][i]));
                    changed |= pin_combo(ui, ("step", i), &mut b.stepper.pins[i]);
                    ui.end_row();
                }
            });
        });

        if self.xtal && self.xtal_text.is_empty() {
            self.xtal_text = format!("{}", b.cpu.xtal_hz / 1e6);
        }
        let mut open = self.xtal;
        window("Quarzfrequenz", &mut open).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.xtal_text).desired_width(90.0));
                ui.label("MHz");
            });
            ui.horizontal(|ui| {
                for mhz in [0.032768, 1.0, 4.0, 8.0, 10.0, 20.0] {
                    if ui.small_button(format!("{mhz}")).clicked() {
                        self.xtal_text = format!("{mhz}");
                    }
                }
            });
            if ui.button("Übernehmen").clicked() {
                if let Ok(v) = self.xtal_text.trim().replace(',', ".").parse::<f64>() {
                    if v > 0.0 {
                        b.cpu.xtal_hz = v * 1e6;
                        changed = true;
                        self.xtal = false;
                    }
                }
            }
        });
        if !open {
            self.xtal = false;
        }
        if !self.xtal {
            self.xtal_text.clear();
        }
        changed
    }

    pub fn show_info(&mut self, ctx: &egui::Context) {
        egui::Window::new("Kurzanleitung").open(&mut self.help).default_width(520.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                ui.label(RichText::new("Ablauf").strong());
                ui.label("1. Quelltext bearbeiten öffnet den Editor. Dort schreibst du das Programm (.src).");
                ui.label("2. Assemblieren & Laden (F5 oder Strg+B) erzeugt LST, HEX, ERR und SYM neben der Quelldatei und lädt das Programm.");
                ui.label("3. Mit dem Steuerpult wird das Programm ausgeführt.");
                ui.add_space(6.0);
                ui.label(RichText::new("Steuerpult").strong());
                egui::Grid::new("keys_help").show(ui, |ui| {
                    for (k, t) in [
                        ("F2", "Reset: Power-On-Reset, PC = 0, bricht auch Go ab"),
                        ("F6", "Step out: bis zum Rücksprung aus dem Unterprogramm"),
                        ("F7", "Ignore: nächsten Befehl überspringen"),
                        ("F8", "Step in: einen Befehl ausführen"),
                        ("F9", "Step over: Unterprogramm in einem Rutsch"),
                        ("F10", "Go: starten bis Breakpoint, nochmal drücken hält an"),
                    ] {
                        ui.label(RichText::new(k).monospace().strong());
                        ui.label(t);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Bedienung").strong());
                ui.label("• Breakpoints: in die erste Spalte des Listings klicken.");
                ui.label("• RAM-Werte anklicken, Hexwert eingeben, Enter.");
                ui.label("• Bits in Status, Option und INTCON sowie Pins der Ports lassen sich anklicken.");
                ui.label("• Rot markierte RAM-Werte haben sich beim letzten Schritt geändert.");
                ui.label("• Die Einstellungen liegen in picsimu.ini. Über Datei → Einstellungen importieren lässt sich die INI des alten Programms übernehmen.");
                ui.add_space(6.0);
                ui.label(RichText::new("Assembler").strong());
                ui.label("Kompatibel zu iL_ASS16: Labels in Spalte 1, EQU, ORG, END, DEVICE, CONFIG, INCLUDE, IF/ELSEIF/ELSE/ENDIF. Zahlen als 35, 0FFH, 0x1F, 01B, 'A'. Bit-Symbole wie TASTE EQU 6,4 für btfsc TASTE. Die Register des 16F84 sind vordefiniert (STATUS, PORTB, GIE …), eigene EQUs gehen vor.");
            });
        });
        egui::Window::new("Info").open(&mut self.about).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading(format!("PicSim {}", env!("CARGO_PKG_VERSION")));
            ui.label("Simulator für den PIC16F84 mit Assembler und Editor.");
            ui.label("Neu geschrieben nach dem Vorbild von PicSimu 0.51.");
            ui.hyperlink_to("github.com/Max-Guenther/picsim", "https://github.com/Max-Guenther/picsim");
        });
    }
}
