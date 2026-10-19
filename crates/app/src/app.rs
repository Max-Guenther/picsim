//! Hauptfenster: Menü, Steuerpult, Simulationsschleife, Dateien.

use crate::apps::Apps;
use crate::editor::{Editor, EditorAction};
use crate::settings::Dialogs;
use crate::update::{UpdateState, Updater};
use eframe::egui;
use picsim_core::{Board, Ini, Listing, Stop};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Standardeinstellungen beim allerersten Start.
const DEFAULT_INI: &str = "\
[LED_ARRAY]
LED0=RB0
LED1=RB1
LED2=RB2
LED3=RB3
LED4=RB4
LED5=RB5
LED6=RB6
LED7=RB7
LED8=nc
DIR1=1
DIR2=0

[SETTINGS]
XTAL=4,000000 MHz

[LCD]
FORMAT_4=1
grau=1
";

pub struct PicSimApp {
    pub board: Board,
    pub lst_path: Option<PathBuf>,
    ini_path: PathBuf,
    pub status: String,
    alert: Option<String>,
    /// Simulation läuft in Echtzeit statt so schnell wie möglich.
    pub realtime: bool,
    realtime_anchor: Option<(Instant, f64)>,
    pub scroll_to_pc: bool,
    /// RAM-Inhalt vor dem letzten Schritt, für die Markierung von Änderungen.
    pub prev_ram: [u8; 256],
    pub ram_edit: Option<(u8, String)>,
    pub seg_frame: [[f32; 8]; 8],
    pub dialogs: Dialogs,
    pub editor: Editor,
    pub apps: Apps,
    updater: Updater,
    pub auto_update: bool,
    zoom: f32,
}

impl PicSimApp {
    pub fn new(cc: &eframe::CreationContext<'_>, arg: Option<String>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Light);
        let ini_path = config_dir().join("picsimu.ini");
        let ini = Ini::load(&ini_path).unwrap_or_else(|_| Ini::parse(DEFAULT_INI));
        let auto_update = ini.get("UPDATE", "AUTO").map(|v| v != "0").unwrap_or(true);
        let zoom = ini.number("DISPLAY", "ZOOM").unwrap_or(1.0) as f32;
        let realtime = ini.flag("SETTINGS", "ECHTZEIT");
        let mut app = PicSimApp {
            board: Board::new(ini),
            lst_path: None,
            ini_path,
            status: "Bereit".into(),
            alert: None,
            realtime,
            realtime_anchor: None,
            scroll_to_pc: true,
            prev_ram: [0; 256],
            ram_edit: None,
            seg_frame: [[0.0; 8]; 8],
            dialogs: Dialogs::default(),
            editor: Editor::default(),
            apps: Apps::default(),
            updater: Updater::default(),
            auto_update,
            zoom,
        };
        cc.egui_ctx.set_zoom_factor(zoom);
        app.apps.load(&app.board.ini);

        // Datei aus der Kommandozeile, sonst die zuletzt geöffnete
        let start_file = arg
            .map(|a| picsim_core::lst::with_default_extension(&a))
            .or_else(|| app.board.ini.get("FILES", "NAME").map(PathBuf::from).filter(|p| p.exists()));
        if let Some(p) = start_file {
            app.open_any(&p);
        }
        if app.editor.path.is_none() {
            if let Some(src) = app.board.ini.get("FILES", "SOURCE").map(PathBuf::from) {
                if src.exists() {
                    app.editor.load(&src);
                }
            }
        }
        if app.auto_update {
            app.updater.check(&cc.egui_ctx, false);
        }
        app
    }

    pub fn save_ini(&mut self) {
        self.board.store_ini();
        self.apps.store(&mut self.board.ini);
        let ini = &mut self.board.ini;
        ini.set_flag("UPDATE", "AUTO", self.auto_update);
        ini.set("DISPLAY", "ZOOM", format!("{:.2}", self.zoom));
        ini.set_flag("SETTINGS", "ECHTZEIT", self.realtime);
        if let Some(p) = &self.lst_path {
            ini.set("FILES", "NAME", p.to_string_lossy());
        }
        if let Some(p) = &self.editor.path {
            ini.set("FILES", "SOURCE", p.to_string_lossy());
        }
        if let Err(e) = ini.save(&self.ini_path) {
            self.status = format!("Einstellungen nicht gespeichert: {e}");
        }
    }

    /// Öffnet LST (Simulation) oder SRC (Editor, wird gleich übersetzt).
    pub fn open_any(&mut self, path: &Path) {
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if ext == "src" || ext == "asm" {
            self.editor.load(path);
            self.editor.open = true;
            if let Some(EditorAction::Loaded(lst)) = self.editor.assemble() {
                self.open_lst(&lst);
            }
        } else {
            self.open_lst(path);
            // Passende Quelldatei gleich im Editor bereitstellen
            for ext in ["src", "SRC", "asm"] {
                let src = path.with_extension(ext);
                if src.exists() {
                    self.editor.load(&src);
                    break;
                }
            }
        }
    }

    pub fn open_lst(&mut self, path: &Path) {
        match Listing::load(path) {
            Ok(l) => {
                let words = l.program().len();
                self.board.load_listing(l);
                self.prev_ram = ram_snapshot(&self.board);
                self.lst_path = Some(path.to_path_buf());
                self.status = format!("{} geladen, {words} Befehle", path.display());
                self.scroll_to_pc = true;
                self.save_ini();
            }
            Err(e) => self.alert = Some(format!("{} lässt sich nicht öffnen: {e}", path.display())),
        }
    }

    fn reload_lst(&mut self) {
        if let Some(p) = self.lst_path.clone() {
            self.open_lst(&p);
        }
    }

    // ------------------------------------------------------------------
    // Steuerung

    fn before_step(&mut self) {
        self.prev_ram = ram_snapshot(&self.board);
        self.alert = None;
    }

    pub fn reset(&mut self) {
        self.board.reset();
        self.prev_ram = ram_snapshot(&self.board);
        self.status = "Reset".into();
        self.scroll_to_pc = true;
    }

    pub fn step_in(&mut self) {
        self.before_step();
        let stop = self.board.step_in();
        self.handle_stop(stop);
    }

    pub fn step_over(&mut self) {
        self.before_step();
        let stop = self.board.step_over();
        self.realtime_anchor = None;
        self.handle_stop(stop);
    }

    pub fn step_out(&mut self) {
        self.before_step();
        self.board.step_out();
        self.realtime_anchor = None;
    }

    pub fn ignore(&mut self) {
        self.before_step();
        self.board.ignore();
        self.scroll_to_pc = true;
    }

    pub fn go(&mut self) {
        self.before_step();
        self.board.go();
        self.realtime_anchor = None;
        self.status = if self.board.running() { "Läuft".into() } else { "Angehalten".into() };
        self.scroll_to_pc = !self.board.running();
    }

    fn handle_stop(&mut self, stop: Stop) {
        match stop {
            Stop::Budget => return,
            Stop::Done => self.status = format!("PC = {:04X}", self.board.cpu.pc),
            Stop::Breakpoint => self.status = format!("Breakpoint bei {:04X}", self.board.cpu.pc),
            Stop::Watchdog => {
                self.status = "Watchdog abgelaufen! RESET".into();
                self.alert = Some("Watchdog abgelaufen! RESET".into());
            }
        }
        self.scroll_to_pc = true;
    }

    /// Lässt die Simulation für einen Bildschirmframe laufen.
    fn run_frame(&mut self, ctx: &egui::Context) {
        if !self.board.running() {
            return;
        }
        let start = Instant::now();
        let budget = Duration::from_millis(14);
        loop {
            if self.realtime {
                let now_us = self.board.cpu.runtime_us();
                let (t0, sim0) = *self.realtime_anchor.get_or_insert((start, now_us));
                if now_us >= sim0 + t0.elapsed().as_secs_f64() * 1e6 {
                    break;
                }
            }
            let stop = self.board.run(2000);
            if stop != Stop::Budget {
                self.handle_stop(stop);
                break;
            }
            if start.elapsed() > budget {
                break;
            }
        }
        if self.board.running() {
            ctx.request_repaint();
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let pressed = |k| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
        if pressed(Key::F2) {
            self.reset();
        }
        if pressed(Key::F6) {
            self.step_out();
        }
        if pressed(Key::F7) {
            self.ignore();
        }
        if pressed(Key::F8) {
            self.step_in();
        }
        if pressed(Key::F9) {
            self.step_over();
        }
        if pressed(Key::F10) {
            self.go();
        }
    }

    // ------------------------------------------------------------------
    // Oberfläche

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("LST-Datei öffnen…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Listing", &["lst", "LST"]).pick_file() {
                        self.open_any(&p);
                    }
                }
                if ui.button("Quelltext öffnen…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Quelltext", &["src", "SRC", "asm"]).pick_file() {
                        self.open_any(&p);
                    }
                }
                if ui.add_enabled(self.lst_path.is_some(), egui::Button::new("Neu laden")).clicked() {
                    self.reload_lst();
                }
                ui.separator();
                if ui.button("Einstellungen importieren…").on_hover_text("picsimu.ini des alten Programms übernehmen").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("INI", &["ini", "INI"]).pick_file() {
                        match Ini::load(&p) {
                            Ok(ini) => {
                                self.apps.load(&ini);
                                self.board.apply_ini(ini);
                                self.save_ini();
                                self.status = format!("Einstellungen aus {} übernommen", p.display());
                            }
                            Err(e) => self.alert = Some(format!("{}: {e}", p.display())),
                        }
                    }
                }
                if ui.button("Einstellungen exportieren…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_file_name("picsimu.ini").save_file() {
                        self.board.store_ini();
                        if let Err(e) = self.board.ini.save(&p) {
                            self.alert = Some(e.to_string());
                        }
                    }
                }
                ui.separator();
                if ui.button("Beenden").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Ansicht", |ui| {
                ui.label("Bildschirm");
                for (name, z) in [("Klein (80 %)", 0.8), ("Normal (100 %)", 1.0), ("Groß (125 %)", 1.25), ("Sehr groß (150 %)", 1.5)] {
                    if ui.radio(self.zoom == z, name).clicked() {
                        self.zoom = z;
                        ui.ctx().set_zoom_factor(z);
                        self.save_ini();
                    }
                }
                ui.separator();
                if ui.checkbox(&mut self.realtime, "Echtzeit").on_hover_text("Simulation läuft so schnell wie der echte PIC").changed() {
                    self.realtime_anchor = None;
                    self.save_ini();
                }
            });
            ui.menu_button("Einstellungen", |ui| self.dialogs.menu(ui));
            if ui.button("Quelltext bearbeiten").clicked() {
                self.editor.open = true;
            }
            ui.menu_button("Applikationen", |ui| self.apps.menu(ui));
            ui.menu_button("Hilfe", |ui| {
                if ui.button("Kurzanleitung").clicked() {
                    self.dialogs.help = true;
                }
                if ui.button("Nach Updates suchen").clicked() {
                    self.updater.check(ui.ctx(), true);
                }
                if ui.checkbox(&mut self.auto_update, "Beim Start nach Updates suchen").changed() {
                    self.save_ini();
                }
            });
            if ui.button("Info").clicked() {
                self.dialogs.about = true;
            }
        });
    }

    fn update_banner(&mut self, ui: &mut egui::Ui) {
        self.updater.poll();
        let state = self.updater.state.clone();
        let show = match &state {
            UpdateState::Available(_) | UpdateState::Installing(_) | UpdateState::Installed(_) => true,
            UpdateState::UpToDate | UpdateState::Failed(_) => self.updater.manual,
            _ => false,
        };
        if !show {
            return;
        }
        egui::Panel::top("update").show(ui, |ui| {
            ui.horizontal(|ui| match state {
                UpdateState::Available(rel) => {
                    ui.label(egui::RichText::new(format!("Version {} ist verfügbar", rel.version)).strong());
                    if ui.button("Jetzt aktualisieren").clicked() {
                        self.updater.install(rel.clone(), ui.ctx());
                    }
                    ui.hyperlink_to("Was ist neu?", &rel.page).on_hover_text(&rel.notes);
                    if ui.button("Später").clicked() {
                        self.updater.state = UpdateState::Idle;
                    }
                }
                UpdateState::Installing(v) => {
                    ui.spinner();
                    ui.label(format!("Version {v} wird geladen…"));
                }
                UpdateState::Installed(v) => {
                    ui.label(format!("Version {v} ist installiert."));
                    if ui.button("Neu starten").clicked() {
                        self.save_ini();
                        crate::update::restart();
                    }
                }
                UpdateState::UpToDate => {
                    ui.label(format!("PicSim {} ist aktuell.", crate::update::CURRENT));
                    if ui.button("OK").clicked() {
                        self.updater.state = UpdateState::Idle;
                    }
                }
                UpdateState::Failed(e) => {
                    ui.colored_label(egui::Color32::DARK_RED, format!("Update: {e}"));
                    if ui.button("OK").clicked() {
                        self.updater.state = UpdateState::Idle;
                    }
                }
                _ => {}
            });
        });
    }

    fn control_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Quarzfrequenz").strong());
            ui.label(format!("{:.6} MHz", self.board.cpu.xtal_hz / 1e6));
            ui.label(format!("{:.3} µs je Zyklus", self.board.cpu.cycle_us()));
            if ui.small_button("ändern…").clicked() {
                self.dialogs.xtal = true;
            }
        });
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Laufzeit").strong());
            ui.label(egui::RichText::new(fmt_runtime(self.board.cpu.runtime_us())).monospace().size(16.0));
            ui.label(format!("{} Zyklen", self.board.cpu.cycles));
            if ui.button("zurücksetzen").clicked() {
                self.board.cpu.cycles = 0;
            }
        });
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Steuerpult").strong());
            let w = ui.available_width();
            let btn = |ui: &mut egui::Ui, text: &str| ui.add_sized([w, 26.0], egui::Button::new(text)).clicked();
            if btn(ui, "Reset (F2)") {
                self.reset();
            }
            if btn(ui, "Ignore (F7)") {
                self.ignore();
            }
            if btn(ui, "Step in (F8)") {
                self.step_in();
            }
            if btn(ui, "Step out (F6)") {
                self.step_out();
            }
            if btn(ui, "Step over (F9)") {
                self.step_over();
            }
            let go = if self.board.running() { "Stop (F10)" } else { "Go (F10)" };
            let go_btn = egui::Button::new(egui::RichText::new(go).strong()).selected(self.board.running());
            if ui.add_sized([w, 32.0], go_btn).clicked() {
                self.go();
            }
            if ui.checkbox(&mut self.realtime, "Echtzeit").changed() {
                self.realtime_anchor = None;
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let file = self.lst_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "keine Datei geladen".into());
            ui.label(egui::RichText::new(format!("Datei: {file}")).small());
            ui.separator();
            ui.label(egui::RichText::new(&self.status).small());
        });
    }

    fn alert_window(&mut self, ctx: &egui::Context) {
        let Some(msg) = self.alert.clone() else { return };
        egui::Window::new("Hinweis")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(msg);
                if ui.button("OK").clicked() {
                    self.alert = None;
                }
            });
    }
}

impl eframe::App for PicSimApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.editor.has_focus(&ctx) {
            self.shortcuts(&ctx);
        }
        self.run_frame(&ctx);
        self.apps.tick(&mut self.board, &ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        self.update_banner(ui);
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("memory")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| crate::view::memory_panel(self, ui));
        egui::Panel::right("control")
            .resizable(false)
            .exact_size(170.0)
            .show(ui, |ui| self.control_panel(ui));
        egui::Panel::bottom("listing")
            .resizable(true)
            .default_size(340.0)
            .show(ui, |ui| crate::view::listing(self, ui));
        egui::CentralPanel::default_margins().show(ui, |ui| crate::periph::peripherals(self, ui));

        if let Some(action) = self.editor.show(&ctx) {
            match action {
                EditorAction::Loaded(lst) => self.open_lst(&lst),
                EditorAction::Saved => self.save_ini(),
            }
        }
        if self.dialogs.show(&ctx, &mut self.board) {
            self.save_ini();
        }
        if self.apps.show(&ctx, &mut self.board) {
            self.save_ini();
        }
        self.dialogs.show_info(&ctx);
        self.alert_window(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_ini();
    }
}

pub fn ram_snapshot(board: &Board) -> [u8; 256] {
    std::array::from_fn(|i| board.cpu.peek(i as u8))
}

fn fmt_runtime(us: f64) -> String {
    if us < 1000.0 {
        format!("{us:.2} µs")
    } else if us < 1_000_000.0 {
        format!("{:.3} ms", us / 1000.0)
    } else {
        format!("{:.4} s", us / 1e6)
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("picsim")
}
