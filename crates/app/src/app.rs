//! Hauptfenster: Menü, Steuerpult, Simulationsschleife, Dateien.

use crate::apps::Apps;
use crate::dialog::{self, FileDialog};
use crate::editor::{Editor, EditorAction};
use crate::layout::Layout;
use crate::project::{self, Project};
use crate::reference::{Reference, Tab};
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
    /// Hier liegen die Arbeitsordner geöffneter Projekte.
    cache_dir: PathBuf,
    pub status: String,
    alert: Option<String>,
    /// Tempo bei Go, Step over und Step out.
    pub speed: Speed,
    /// Befehle je Sekunde im Tempo Langsam.
    pub slow_ips: f64,
    realtime_anchor: Option<(Instant, f64)>,
    slow_credit: f64,
    last_frame: Option<Instant>,
    pub scroll_to_pc: bool,
    /// RAM-Inhalt vor dem letzten Schritt, für die Markierung von Änderungen.
    pub prev_ram: [u8; 256],
    pub ram_edit: Option<(u8, String)>,
    pub seg_frame: [[f32; 8]; 8],
    pub dialogs: Dialogs,
    pub editor: Editor,
    pub apps: Apps,
    pub layout: Layout,
    reference: Reference,
    updater: Updater,
    pub auto_update: bool,
    zoom: f32,
    file_dialog: Option<FileDialog<AppDialog>>,
    /// Geöffnete oder zuletzt gespeicherte Projektdatei.
    project: Option<PathBuf>,
    theme: egui::ThemePreference,
}

/// Wofür der Dateidialog im Menü Datei geöffnet wurde.
enum AppDialog {
    Open,
    SaveProject,
    ImportIni,
    ExportIni,
}

impl PicSimApp {
    pub fn new(cc: &eframe::CreationContext<'_>, arg: Option<String>) -> Self {
        crate::assoc::install(&cc.egui_ctx);
        let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("picsim");
        Self::with_dirs(cc, arg, config_dir(), cache)
    }

    /// Wie `new`, mit eigenem Ordner für die ini und den Cache (für Tests).
    fn with_dirs(cc: &eframe::CreationContext<'_>, arg: Option<String>, config: PathBuf, cache_dir: PathBuf) -> Self {
        crate::editor::install_tab_fonts(&cc.egui_ctx);
        let ini_path = config.join("picsimu.ini");
        let ini = Ini::load(&ini_path).unwrap_or_else(|_| Ini::parse(DEFAULT_INI));
        let theme = crate::theme::from_ini(&ini);
        cc.egui_ctx.set_theme(theme);
        let auto_update = ini.get("UPDATE", "AUTO").map(|v| v != "0").unwrap_or(true);
        let zoom = ini.number("DISPLAY", "ZOOM").unwrap_or(1.0) as f32;
        let (speed, slow_ips) = speed_from_ini(&ini);
        let mut app = PicSimApp {
            board: Board::new(ini),
            lst_path: None,
            ini_path,
            cache_dir,
            status: "Bereit".into(),
            alert: None,
            speed,
            slow_ips,
            realtime_anchor: None,
            slow_credit: 0.0,
            last_frame: None,
            scroll_to_pc: true,
            prev_ram: [0; 256],
            ram_edit: None,
            seg_frame: [[0.0; 8]; 8],
            dialogs: Dialogs::default(),
            editor: Editor::default(),
            apps: Apps::default(),
            layout: Layout::default(),
            reference: Reference::default(),
            updater: Updater::default(),
            auto_update,
            zoom,
            file_dialog: None,
            project: None,
            theme,
        };
        cc.egui_ctx.set_zoom_factor(zoom);
        app.load_tab_width();
        // Zuletzt benutztes Projekt bleibt zugeordnet, die Einstellungen stehen schon in der ini
        app.project = app.board.ini.get("FILES", "PROJEKT").map(PathBuf::from).filter(|p| p.exists());
        app.apps.load(&app.board.ini);
        app.layout = Layout::load(&app.board.ini);
        app.reference.open = app.board.ini.flag("Applikationen", "Nachschlagen");
        if app.board.ini.get("Applikationen", "NachschlagenReiter") == Some("Register") {
            app.reference.tab = Tab::Register;
        }

        // Datei aus der Kommandozeile, sonst die zuletzt geöffnete
        let start_file = arg
            .map(|a| picsim_core::lst::with_default_extension(&a))
            .or_else(|| {
                app.board
                    .ini
                    .get("FILES", "NAME")
                    .map(PathBuf::from)
                    .filter(|p| p.exists())
            });
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
        self.layout.store(&mut self.board.ini);
        self.board.ini.set_flag("Applikationen", "Nachschlagen", self.reference.open);
        let tab = if self.reference.tab == Tab::Register { "Register" } else { "Befehle" };
        self.board.ini.set("Applikationen", "NachschlagenReiter", tab);
        let ini = &mut self.board.ini;
        ini.set_flag("UPDATE", "AUTO", self.auto_update);
        ini.set("DISPLAY", "ZOOM", format!("{:.2}", self.zoom));
        ini.set("SETTINGS", "TEMPO", self.speed.name());
        ini.set("SETTINGS", "LANGSAM", format!("{}", self.slow_ips));
        ini.set("EDITOR", "TABBREITE", self.editor.tab_width.to_string());
        crate::theme::store(ini, self.theme);
        if let Some(p) = &self.lst_path {
            ini.set("FILES", "NAME", p.to_string_lossy());
        }
        if let Some(p) = &self.editor.path {
            ini.set("FILES", "SOURCE", p.to_string_lossy());
        }
        match &self.project {
            Some(p) => ini.set("FILES", "PROJEKT", p.to_string_lossy()),
            None => ini.set("FILES", "PROJEKT", ""),
        }
        if let Err(e) = ini.save(&self.ini_path) {
            self.status = format!("Einstellungen nicht gespeichert: {e}");
        }
    }

    /// Öffnet LST (Simulation) oder SRC (Editor, wird gleich übersetzt).
    pub fn open_any(&mut self, path: &Path) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if ext == project::EXT {
            self.open_project(path);
        } else if ext == "src" || ext == "asm" {
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

    fn load_tab_width(&mut self) {
        if let Some(n) = self.board.ini.number("EDITOR", "TABBREITE") {
            self.editor.tab_width = (n as usize).clamp(2, crate::editor::MAX_TAB);
        }
    }

    /// Projekt öffnen: Dateien in den Arbeitsordner, Einstellungen
    /// übernehmen, Quelltext übersetzen und laden.
    pub fn open_project(&mut self, path: &Path) {
        let p = match Project::read(path) {
            Ok(p) => p,
            Err(e) => {
                self.alert = Some(format!("Projekt lässt sich nicht öffnen: {e}"));
                return;
            }
        };
        let work = project::workdir(path, &self.cache_dir);
        if let Err(e) = p.extract(&work) {
            self.alert = Some(format!("Projekt lässt sich nicht entpacken nach {}: {e}", work.display()));
            return;
        }
        let ini = p.settings(&self.board.ini);
        self.apps.load(&ini);
        self.layout = Layout::load(&ini);
        (self.speed, self.slow_ips) = speed_from_ini(&ini);
        self.board.apply_ini(ini);
        self.load_tab_width();
        if let Some(s) = p.ini.get(project::STATE, "Schalter") {
            let mut pos = self.board.switch_pos;
            project::switches_from_text(s, &mut pos);
            for (i, &v) in pos.iter().enumerate() {
                self.board.set_switch(i, v);
            }
        }
        self.project = Some(path.to_path_buf());
        if let Some(src) = p.source() {
            self.editor.load(&work.join(src));
            self.editor.open = true;
            if let Some(EditorAction::Loaded(lst)) = self.editor.assemble() {
                self.open_lst(&lst);
            }
        } else if let Some(lst) = p.listing() {
            self.open_lst(&work.join(lst));
        }
        if let Some(bp) = p.ini.get(project::STATE, "Haltepunkte") {
            self.board.breakpoints = project::breakpoints_from_text(bp);
        }
        self.status = format!("Projekt {} geöffnet", path.display());
        self.save_ini();
    }

    /// Alles, was zum Projekt gehört, in `path` schreiben.
    fn save_project(&mut self, path: &Path) {
        if !self.editor.save_if_dirty() {
            self.alert = Some("Der Quelltext ließ sich nicht speichern, das Projekt wurde nicht geschrieben.".into());
            return;
        }
        self.save_ini();
        let mut p = Project::new(&self.board.ini);
        let added = if let Some(src) = self.editor.path.clone() {
            let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "programm.src".into());
            p.add_source(&name, self.editor.file_bytes(), src.parent())
        } else if !self.editor.is_template() {
            p.add_source("programm.src", self.editor.file_bytes(), None)
        } else if let Some(lst) = self.lst_path.clone() {
            p.add_listing(&lst).map_err(|e| format!("{}: {e}", lst.display()))
        } else {
            Ok(())
        };
        if let Err(e) = added {
            self.alert = Some(e);
            return;
        }
        p.ini.set(project::STATE, "Schalter", project::switches_to_text(&self.board.switch_pos));
        p.ini.set(project::STATE, "Haltepunkte", project::breakpoints_to_text(&self.board.breakpoints));
        match p.write(path) {
            Ok(()) => {
                self.project = Some(path.to_path_buf());
                self.status = format!("Projekt {} gespeichert", path.display());
                self.save_ini();
            }
            Err(e) => self.alert = Some(format!("Projekt nicht gespeichert: {e}")),
        }
    }

    fn save_project_as(&mut self) {
        let name = self
            .project
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| self.editor.path.as_ref().and_then(|p| p.file_stem()).map(|s| format!("{}.{}", s.to_string_lossy(), project::EXT)))
            .unwrap_or_else(|| format!("projekt.{}", project::EXT));
        let dlg = rfd::AsyncFileDialog::new().add_filter("PicSim-Projekt", &[project::EXT]).set_file_name(name);
        self.start_dialog(FileDialog::save(dlg, AppDialog::SaveProject));
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
        self.status = if self.board.running() {
            "Läuft".into()
        } else {
            "Angehalten".into()
        };
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
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0)
            .min(0.1);
        self.last_frame = Some(now);
        if !self.board.running() {
            self.slow_credit = 0.0;
            return;
        }
        match self.speed {
            Speed::Langsam => {
                self.slow_credit += dt * self.slow_ips;
                let steps = self.slow_credit.floor();
                self.slow_credit -= steps;
                if steps >= 1.0 {
                    self.before_step();
                    let stop = self.board.run(steps as usize);
                    self.handle_stop(stop);
                }
            }
            Speed::Maximal | Speed::Echtzeit => {
                let budget = Duration::from_millis(14);
                loop {
                    if self.speed == Speed::Echtzeit {
                        let now_us = self.board.cpu.runtime_us();
                        let (t0, sim0) = *self.realtime_anchor.get_or_insert((now, now_us));
                        if now_us >= sim0 + t0.elapsed().as_secs_f64() * 1e6 {
                            break;
                        }
                    }
                    // Kleine Blöcke mit ungerader Länge, damit ein Bild nicht immer
                    // an derselben Stelle einer Programmschleife endet
                    let stop = self.board.run(97);
                    if stop != Stop::Budget {
                        self.handle_stop(stop);
                        break;
                    }
                    if now.elapsed() > budget {
                        break;
                    }
                }
            }
        }
        if self.board.running() {
            ctx.request_repaint();
        }
    }

    fn speed_choice(&mut self, ui: &mut egui::Ui) {
        for s in [Speed::Maximal, Speed::Echtzeit, Speed::Langsam] {
            if ui
                .radio(self.speed == s, s.name())
                .on_hover_text(s.hint())
                .clicked()
            {
                self.speed = s;
                self.realtime_anchor = None;
                self.save_ini();
            }
        }
        if self.speed == Speed::Langsam {
            let r = ui.add(
                egui::DragValue::new(&mut self.slow_ips)
                    .range(0.5..=1000.0)
                    .speed(0.5)
                    .suffix(" Befehle/s"),
            );
            if r.changed() {
                self.save_ini();
            }
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

    fn start_dialog(&mut self, d: FileDialog<AppDialog>) {
        if self.file_dialog.is_none() {
            self.file_dialog = Some(d);
        }
    }

    /// Ergebnis eines fertigen Dateidialogs aus dem Menü Datei verarbeiten.
    fn finish_dialog(&mut self, ctx: &egui::Context) {
        let Some((purpose, Some(mut p))) = dialog::poll(&mut self.file_dialog, ctx) else { return };
        match purpose {
            AppDialog::Open => {
                // Einzelne Datei statt Projekt: das Projekt ist dann nicht mehr offen
                if p.extension().is_none_or(|e| e != project::EXT) {
                    self.project = None;
                }
                self.open_any(&p)
            }
            AppDialog::SaveProject => {
                if p.extension().is_none() {
                    p.set_extension(project::EXT);
                }
                self.save_project(&p);
            }
            AppDialog::ImportIni => match Ini::load(&p) {
                Ok(ini) => {
                    self.apps.load(&ini);
                    self.board.apply_ini(ini);
                    self.save_ini();
                    self.status = format!("Einstellungen aus {} übernommen", p.display());
                }
                Err(e) => self.alert = Some(format!("{}: {e}", p.display())),
            },
            AppDialog::ExportIni => {
                self.board.store_ini();
                if let Err(e) = self.board.ini.save(&p) {
                    self.alert = Some(e.to_string());
                }
            }
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("Projekt öffnen…").clicked() {
                    let dlg = rfd::AsyncFileDialog::new().add_filter("PicSim-Projekt", &[project::EXT]);
                    self.start_dialog(FileDialog::open(dlg, AppDialog::Open));
                }
                let save = ui.button("Projekt speichern").on_hover_text(
                    "Quelltext, Einstellungen, Layout, Schalter und Haltepunkte in eine Datei, die du weitergeben kannst",
                );
                if save.clicked() {
                    match self.project.clone() {
                        Some(p) => self.save_project(&p),
                        None => self.save_project_as(),
                    }
                }
                if ui.button("Projekt speichern unter…").clicked() {
                    self.save_project_as();
                }
                ui.separator();
                if ui.button("LST-Datei öffnen…").clicked() {
                    let dlg = rfd::AsyncFileDialog::new().add_filter("Listing", &["lst", "LST"]);
                    self.start_dialog(FileDialog::open(dlg, AppDialog::Open));
                }
                if ui.button("Quelltext öffnen…").clicked() {
                    let dlg = rfd::AsyncFileDialog::new().add_filter("Quelltext", &["src", "SRC", "asm"]);
                    self.start_dialog(FileDialog::open(dlg, AppDialog::Open));
                }
                if ui
                    .add_enabled(self.lst_path.is_some(), egui::Button::new("Neu laden"))
                    .clicked()
                {
                    self.reload_lst();
                }
                ui.separator();
                if ui
                    .button("Einstellungen importieren…")
                    .on_hover_text("picsimu.ini des alten Programms übernehmen")
                    .clicked()
                {
                    let dlg = rfd::AsyncFileDialog::new().add_filter("INI", &["ini", "INI"]);
                    self.start_dialog(FileDialog::open(dlg, AppDialog::ImportIni));
                }
                if ui.button("Einstellungen exportieren…").clicked() {
                    let dlg = rfd::AsyncFileDialog::new().set_file_name("picsimu.ini");
                    self.start_dialog(FileDialog::save(dlg, AppDialog::ExportIni));
                }
                ui.separator();
                if ui.button("Beenden").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Ansicht", |ui| {
                if self.layout.menu(ui) {
                    self.save_ini();
                }
                ui.separator();
                ui.label("Schriftgröße (auch Strg + und Strg -)");
                for (name, z) in [
                    ("Klein (80 %)", 0.8),
                    ("Normal (100 %)", 1.0),
                    ("Groß (125 %)", 1.25),
                    ("Sehr groß (150 %)", 1.5),
                ] {
                    if ui.radio(self.zoom == z, name).clicked() {
                        self.zoom = z;
                        ui.ctx().set_zoom_factor(z);
                        self.save_ini();
                    }
                }
                ui.separator();
                ui.label("Tempo bei Go");
                self.speed_choice(ui);
            });
            ui.menu_button("Einstellungen", |ui| {
                self.dialogs.menu(ui);
                ui.separator();
                let mut free = self.layout.is_free();
                if ui
                    .checkbox(&mut free, "Fenster frei verschieben")
                    .on_hover_text(
                        "An: Jeder Bereich lässt sich verschieben und in der Größe ändern.\n\
                         Aus: Alle Bereiche liegen fest angeordnet im Programmfenster.",
                    )
                    .changed()
                {
                    self.layout.set_free(free);
                    self.save_ini();
                }
                ui.separator();
                if crate::theme::menu(ui, &mut self.theme) {
                    ui.ctx().set_theme(self.theme);
                    self.save_ini();
                }
            });
            if ui.button("Quelltext bearbeiten").clicked() {
                self.editor.open = true;
            }
            ui.menu_button("Applikationen", |ui| self.apps.menu(ui));
            ui.menu_button("Hilfe", |ui| {
                if ui.button("Kurzanleitung").clicked() {
                    self.dialogs.help = true;
                }
                if ui.button("Befehlsübersicht").clicked() {
                    self.reference.open = true;
                    self.reference.tab = Tab::Befehle;
                }
                if ui.button("Registerübersicht (Tabelle 4-1)").clicked() {
                    self.reference.open = true;
                    self.reference.tab = Tab::Register;
                }
                if ui.button("Nach Updates suchen").clicked() {
                    self.updater.check(ui.ctx(), true);
                }
                if ui
                    .checkbox(&mut self.auto_update, "Beim Start nach Updates suchen")
                    .changed()
                {
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
            UpdateState::Available(_) | UpdateState::Installing(_) | UpdateState::Installed(_) => {
                true
            }
            UpdateState::UpToDate | UpdateState::Failed(_) => self.updater.manual,
            _ => false,
        };
        if !show {
            return;
        }
        egui::Panel::top("update").show(ui, |ui| {
            ui.horizontal(|ui| match state {
                UpdateState::Available(rel) => {
                    ui.label(
                        egui::RichText::new(format!("Version {} ist verfügbar", rel.version))
                            .strong(),
                    );
                    if ui.button("Jetzt aktualisieren").clicked() {
                        self.updater.install(rel.clone(), ui.ctx());
                    }
                    ui.hyperlink_to("Was ist neu?", &rel.page)
                        .on_hover_text(&rel.notes);
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
                    ui.colored_label(crate::theme::pick(ui, egui::Color32::DARK_RED, egui::Color32::LIGHT_RED), format!("Update: {e}"));
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
            ui.label(
                egui::RichText::new(fmt_runtime(self.board.cpu.runtime_us()))
                    .monospace()
                    .size(16.0),
            );
            ui.label(format!("{} Zyklen", self.board.cpu.cycles));
            if ui.button("zurücksetzen").clicked() {
                self.board.cpu.cycles = 0;
            }
        });
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Steuerpult").strong());
            let w = ui.available_width();
            let btn = |ui: &mut egui::Ui, text: &str| {
                ui.add_sized([w, 26.0], egui::Button::new(text)).clicked()
            };
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
            let go = if self.board.running() {
                "Stop (F10)"
            } else {
                "Go (F10)"
            };
            let go_btn =
                egui::Button::new(egui::RichText::new(go).strong()).selected(self.board.running());
            if ui.add_sized([w, 32.0], go_btn).clicked() {
                self.go();
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Tempo").small());
            self.speed_choice(ui);
        });
    }

    /// Freier Modus: jeder Bereich ist ein eigenes, verschiebbares Fenster.
    fn free_windows(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Fläche für die Fenster: alles zwischen Menü und Statuszeile
        let area = egui::CentralPanel::default_margins()
            .show(ui, |ui| ui.max_rect())
            .inner;
        // Größe geändert: Fenster erst anteilig neu aufbauen, wenn sie kurz stillsteht
        if self.layout.update(area) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        let layout_area = self.layout.area(area);
        for i in 0..crate::layout::N {
            if !self.layout.visible[i] {
                continue;
            }
            let mut open = true;
            let mut inner = None;
            let shown = self.layout.window(i, layout_area).open(&mut open).show(ctx, |ui| {
                inner = Some(ui.max_rect());
                self.window_content(i, ui);
            });
            if let Some(r) = shown {
                let by_user = ctx.input(|i| i.pointer.any_down() || i.pointer.any_released());
                self.layout.record(i, r.response.rect, inner, by_user);
            }
            if !open {
                self.layout.visible[i] = false;
                self.save_ini();
            }
        }
    }

    /// Fester Modus: alle Bereiche als Kacheln in vorgegebener Anordnung,
    /// die Trenner dazwischen lassen sich ziehen.
    fn fixed_tiles(&mut self, ui: &mut egui::Ui) {
        let area = ui.max_rect();
        let (mut tiles, mut handles) = (Vec::new(), Vec::new());
        self.layout.fixed.arrange(area, &self.layout.visible, &mut tiles, &mut handles);
        for (i, rect) in tiles {
            let rect = rect.shrink(2.0);
            let mut tile = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(("kachel", i)));
            tile.set_clip_rect(rect.intersect(ui.clip_rect()));
            egui::Frame::group(tile.style()).show(&mut tile, |ui| {
                ui.set_min_size(ui.available_size());
                ui.label(egui::RichText::new(crate::layout::WINDOWS[i].1).strong());
                self.window_content(i, ui);
            });
        }
        // Trenner zuletzt, damit sie über dem Rand der Kacheln liegen
        let mut stopped = false;
        for h in &handles {
            let r = ui.interact(h.rect, egui::Id::unique(("trenner", h.key, h.a)), egui::Sense::drag());
            if r.hovered() || r.dragged() {
                let icon = if h.row { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical };
                ui.ctx().set_cursor_icon(icon);
                let line = if h.row {
                    [h.rect.center_top(), h.rect.center_bottom()]
                } else {
                    [h.rect.left_center(), h.rect.right_center()]
                };
                ui.painter().line_segment(line, egui::Stroke::new(3.0, ui.visuals().selection.bg_fill));
            }
            if r.dragged() {
                let d = r.drag_delta();
                self.layout.fixed.drag(h, if h.row { d.x } else { d.y });
            }
            stopped |= r.drag_stopped();
        }
        // Ecken zuletzt: dort greift man beide Trenner zugleich
        for c in crate::tiles::corners(&handles) {
            let (v, h) = (&handles[c.v], &handles[c.h]);
            let r = ui.interact(c.rect, egui::Id::unique(("ecke", v.key, v.a, h.key, h.a)), egui::Sense::drag());
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                let stroke = egui::Stroke::new(3.0, ui.visuals().selection.bg_fill);
                ui.painter().line_segment([v.rect.center_top(), v.rect.center_bottom()], stroke);
                ui.painter().line_segment([h.rect.left_center(), h.rect.right_center()], stroke);
            }
            if r.dragged() {
                let d = r.drag_delta();
                self.layout.fixed.drag(v, d.x);
                self.layout.fixed.drag(h, d.y);
            }
            stopped |= r.drag_stopped();
        }
        if stopped {
            self.save_ini();
        }
        self.layout.grips = handles;
    }

    /// Beim ersten Start fragen, wie die Bereiche angeordnet sein sollen.
    fn mode_dialog(&mut self, ctx: &egui::Context) {
        if self.layout.free.is_some() {
            return;
        }
        let mut choice = None;
        egui::Window::new("Fensteranordnung")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label("Wie sollen die Bereiche angeordnet sein?");
                ui.add_space(8.0);
                if ui.add(egui::Button::new(egui::RichText::new("Fest angeordnet").strong()).min_size(egui::vec2(180.0, 28.0))).clicked() {
                    choice = Some(false);
                }
                ui.label("Alle Bereiche liegen an festen Plätzen und wachsen mit dem Programmfenster mit.");
                ui.add_space(8.0);
                if ui.add(egui::Button::new(egui::RichText::new("Frei verschiebbar").strong()).min_size(egui::vec2(180.0, 28.0))).clicked() {
                    choice = Some(true);
                }
                ui.label("Jeder Bereich ist ein eigenes Fenster, das du verschieben und größer oder kleiner ziehen kannst.");
                ui.add_space(8.0);
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "Du kannst das jederzeit unter Einstellungen > Fenster frei verschieben ändern.",
                    )
                    .weak(),
                );
            });
        if let Some(free) = choice {
            self.layout.set_free(free);
            self.save_ini();
        }
    }

    fn window_content(&mut self, i: usize, ui: &mut egui::Ui) {
        match crate::layout::WINDOWS[i].0 {
            "ram" => crate::view::ram(self, ui),
            "sfr" => crate::view::sfr(self, ui),
            "ports" => crate::periph::ports(self, ui),
            "lcd" => crate::periph::lcd(self, ui),
            "sseg" => crate::periph::seven_seg(self, ui),
            "switches" => crate::periph::switches(self, ui),
            "keys" => crate::periph::keys(self, ui),
            "leds" => crate::periph::leds(self, ui),
            "stepper" => crate::periph::stepper(self, ui),
            "fgen" => crate::periph::funcgen(self, ui),
            "listing" => crate::view::listing(self, ui),
            _ => {
                egui::ScrollArea::vertical()
                    .id_salt("control")
                    .show(ui, |ui| self.control_panel(ui));
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let file = self
                .lst_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "keine Datei geladen".into());
            if let Some(p) = &self.project {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ui.label(egui::RichText::new(format!("Projekt: {name}")).small()).on_hover_text(p.display().to_string());
                ui.separator();
            }
            ui.label(egui::RichText::new(format!("Datei: {file}")).small());
            ui.separator();
            ui.label(egui::RichText::new(&self.status).small());
        });
    }

    fn alert_window(&mut self, ctx: &egui::Context) {
        let Some(msg) = self.alert.clone() else {
            return;
        };
        egui::Window::new("Hinweis").order(egui::Order::Foreground)
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
        self.finish_dialog(&ctx);
        // macOS: im Finder doppelgeklickte Dateien
        for p in crate::assoc::take_opened() {
            self.open_any(&p);
        }
        if !self.editor.has_focus(&ctx) {
            self.shortcuts(&ctx);
        }
        self.run_frame(&ctx);
        self.apps.tick(&mut self.board, &ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        self.update_banner(ui);
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.layout.is_free() {
            self.free_windows(ui, &ctx);
        } else {
            egui::CentralPanel::default_margins().show(ui, |ui| self.fixed_tiles(ui));
        }
        self.mode_dialog(&ctx);

        if let Some(action) = self.editor.show(&ctx) {
            match action {
                EditorAction::Loaded(lst) => self.open_lst(&lst),
                EditorAction::Saved => match self.project.clone() {
                    // Speichern des Projekt-Quelltexts aktualisiert auch das Projekt
                    Some(p) if self.editor.path.as_ref().is_some_and(|e| e.starts_with(project::workdir(&p, &self.cache_dir))) => {
                        self.save_project(&p)
                    }
                    _ => self.save_ini(),
                },
                EditorAction::ShowReference => {
                    self.reference.open = true;
                    self.reference.tab = Tab::Befehle;
                }
            }
        }
        if self.dialogs.show(&ctx, &mut self.board) {
            self.save_ini();
        }
        if self.apps.show(&ctx, &mut self.board) {
            self.save_ini();
        }
        if let Some((text, instruction)) = self.reference.show(&ctx, &self.board.cpu) {
            self.editor.insert(&ctx, &text, instruction);
        }
        self.dialogs.show_info(&ctx);
        self.alert_window(&ctx);
    }

    /// Fensterlagen kommen aus der INI (als Anteile), nicht aus egui.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_ini();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Maximal,
    Echtzeit,
    Langsam,
}

impl Speed {
    fn name(self) -> &'static str {
        match self {
            Speed::Maximal => "Maximal",
            Speed::Echtzeit => "Echtzeit",
            Speed::Langsam => "Langsam",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Speed::Maximal => "So schnell wie möglich",
            Speed::Echtzeit => "So schnell wie der echte PIC mit dem eingestellten Quarz",
            Speed::Langsam => "Wenige Befehle je Sekunde, die grüne Zeile wandert sichtbar mit",
        }
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

fn speed_from_ini(ini: &Ini) -> (Speed, f64) {
    let speed = match ini.get("SETTINGS", "TEMPO") {
        Some("Echtzeit") => Speed::Echtzeit,
        Some("Langsam") => Speed::Langsam,
        Some(_) => Speed::Maximal,
        None if ini.flag("SETTINGS", "ECHTZEIT") => Speed::Echtzeit,
        None => Speed::Maximal,
    };
    let slow_ips = ini.number("SETTINGS", "LANGSAM").unwrap_or(10.0).clamp(0.5, 1000.0);
    (speed, slow_ips)
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("picsim")
}

#[cfg(test)]
mod tests {
    use super::*;
    use picsim_core::SwitchPos;

    /// App wie auf einem eigenen Rechner: eigene ini, eigener Cache, kein Update-Check.
    fn machine(root: &Path, name: &str) -> PicSimApp {
        machine_ctx(root, name, &egui::Context::default())
    }

    fn machine_ctx(root: &Path, name: &str, ctx: &egui::Context) -> PicSimApp {
        let config = root.join(name).join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("picsimu.ini"), "[UPDATE]\nAUTO=0\n").unwrap();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        PicSimApp::with_dirs(&cc, None, config, root.join(name).join("cache"))
    }

    /// Ein Bild der ganzen App mit den gegebenen Eingaben zeichnen.
    fn frame(ctx: &egui::Context, app: &mut PicSimApp, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let mut out = ctx.run_ui(input, |ui| eframe::App::ui(app, ui, &mut f));
        out.textures_delta.clear();
    }

    #[test]
    fn dragging_a_corner_resizes_width_and_height() {
        use egui::{Event, PointerButton};
        let root = std::env::temp_dir().join(format!("picsim-ecke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ctx = egui::Context::default();
        let mut app = machine_ctx(&root, "a", &ctx);
        app.layout.set_free(false);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);

        // Ecke rechts unten am Schalter-Bereich
        let grips = &app.layout.grips;
        let v = grips.iter().find(|h| h.key == "geraete" && h.a == 0).unwrap();
        let h = grips.iter().find(|h| h.key == "mitte" && h.a == 1).unwrap();
        let start = egui::pos2(v.rect.center().x, h.rect.center().y);
        // Größe des Schalter-Bereichs bei einer festen Fläche
        let size = |app: &PicSimApp| {
            let (mut tiles, mut hs) = (Vec::new(), Vec::new());
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
            app.layout.fixed.arrange(area, &app.layout.visible, &mut tiles, &mut hs);
            tiles.iter().find(|(i, _)| *i == 5).unwrap().1.size()
        };
        let before = size(&app);
        let button = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=10 {
            frame(&ctx, &mut app, vec![Event::PointerMoved(start + egui::vec2(step as f32 * 6.0, step as f32 * 4.0))]);
        }
        frame(&ctx, &mut app, vec![button(start + egui::vec2(60.0, 40.0), false)]);
        let after = size(&app);
        assert!(after.x > before.x + 5.0, "Schalter breiter: {before:?} -> {after:?}");
        assert!(after.y > before.y + 5.0, "Schalter höher: {before:?} -> {after:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dragging_a_divider_with_the_mouse_resizes_tiles() {
        use egui::{Event, PointerButton};
        let root = std::env::temp_dir().join(format!("picsim-drag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ctx = egui::Context::default();
        let mut app = machine_ctx(&root, "a", &ctx);
        app.layout.set_free(false);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);

        let grip = app.layout.grips.iter().find(|h| h.key == "spalten" && h.a == 0).expect("Trenner gezeichnet");
        let start = grip.rect.center();
        let before = app.layout.fixed.sizes.clone();
        let button = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=10 {
            frame(&ctx, &mut app, vec![Event::PointerMoved(start + egui::vec2(step as f32 * 15.0, 0.0))]);
        }
        let end = start + egui::vec2(150.0, 0.0);
        frame(&ctx, &mut app, vec![button(end, false)]);
        frame(&ctx, &mut app, vec![]);

        let after = &app.layout.fixed.sizes;
        assert!(after[0] > before[0] + 0.05, "linke Spalte breiter: {before:?} -> {after:?}");
        assert!((after[2] - before[2]).abs() < 1e-6, "Steuerpult unverändert");
        let moved = app.layout.grips.iter().find(|h| h.key == "spalten" && h.a == 0).unwrap().rect.center();
        assert!((moved.x - end.x).abs() < 3.0, "Trenner folgt der Maus: {} statt {}", moved.x, end.x);
        // Beim Loslassen gespeichert
        let ini = Ini::load(&app.ini_path).unwrap();
        assert!(ini.get("FENSTER", "fest_spalten").is_some());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn project_runs_the_same_on_another_machine() {
        let root = std::env::temp_dir().join(format!("picsim-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src_dir = root.join("quellen");
        std::fs::create_dir_all(src_dir.join("inc")).unwrap();
        std::fs::write(src_dir.join("inc/regs.inc"), "status\tequ\t3\nportb\tequ\t6\n").unwrap();
        let src = "\tinclude \"inc/regs.inc\"\n\torg\t0\nloop\n\tincf\tportb,f\n\tgoto\tloop\n\tend\n";
        std::fs::write(src_dir.join("blink.src"), src).unwrap();

        // Rechner A: Quelltext laden, einiges einstellen, als Projekt speichern
        let mut a = machine(&root, "a");
        a.open_any(&src_dir.join("blink.src"));
        assert!(a.lst_path.is_some(), "Quelltext übersetzt und geladen");
        a.board.set_switch(0, SwitchPos::High);
        a.board.set_switch(5, SwitchPos::Low);
        a.board.switch_names[0] = "Start".into();
        a.board.breakpoints.insert(1);
        a.editor.tab_width = 12;
        a.layout.set_free(false);
        a.layout.fixed.sizes = vec![0.3, 0.57, 0.13];
        a.speed = Speed::Langsam;
        a.slow_ips = 25.0;
        let file = root.join("blink.picsim");
        a.save_project(&file);
        assert_eq!(a.alert, None);
        assert!(file.exists());

        // Rechner B: frische Installation, nur die Projektdatei
        let mut b = machine(&root, "b");
        b.open_project(&file);
        assert_eq!(b.alert, None);
        assert_eq!(b.editor.text, src, "Quelltext gleich");
        assert!(!b.editor.dirty());
        let lst = b.lst_path.clone().expect("Listing geladen");
        assert!(lst.starts_with(root.join("b/cache")), "Ausgaben landen im Arbeitsordner, nicht beim Absender");
        assert_eq!(b.board.listing.program(), a.board.listing.program(), "gleiches Programm");
        assert_eq!(b.board.switch_pos[0], SwitchPos::High);
        assert_eq!(b.board.switch_pos[5], SwitchPos::Low);
        assert_eq!(b.board.switch_names[0], "Start");
        assert!(b.board.breakpoints.contains(&1));
        assert_eq!(b.editor.tab_width, 12);
        assert!(!b.layout.is_free());
        assert!((b.layout.fixed.sizes[0] - 0.3).abs() < 1e-3);
        assert!(matches!(b.speed, Speed::Langsam));
        assert!((b.slow_ips - 25.0).abs() < 1e-9);
        assert_eq!(b.board.ini.get("UPDATE", "AUTO"), Some("0"), "eigene Update-Einstellung bleibt");
        assert_eq!(b.project.as_deref(), Some(file.as_path()));

        // B ändert den Quelltext und speichert im Editor: das Projekt wird aktualisiert
        b.editor.text = src.replace("incf", "decf");
        assert!(b.editor.save_if_dirty());
        let p = b.project.clone().unwrap();
        b.save_project(&p);
        let mut c = machine(&root, "c");
        c.open_project(&file);
        assert!(c.editor.text.contains("decf"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
