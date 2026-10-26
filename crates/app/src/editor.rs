//! Eingebauter Editor für PIC-Assembler mit Syntax-Hervorhebung,
//! Zeilennummern, Fehleranzeige und "Assemblieren & Laden".

use crate::dialog::{self, FileDialog};
use eframe::egui::{self, text::LayoutJob, Color32, FontId, RichText, TextFormat};
use picsim_asm::AsmError;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = "\
; Neues Programm für den PIC16F84
;
; Die Register des 16F84 sind vordefiniert (STATUS, PORTA, PORTB, TRISB, …).

\tdevice\t16f84

zaehler\tequ\t20h

\torg\t0
start
\tbsf\tSTATUS,RP0\t; Bank 1
\tclrf\tTRISB\t\t; Port B als Ausgang
\tbcf\tSTATUS,RP0\t; Bank 0
loop
\tincf\tPORTB,f
\tgoto\tloop

\tend
";

pub const DEFAULT_TAB: usize = 8;
pub const MAX_TAB: usize = 16;

/// Schriftfamilie, deren Tabzeichen genau `n` Zeichen breit ist.
fn tab_family(n: usize) -> egui::FontFamily {
    egui::FontFamily::Name(format!("tab{n}").into())
}

/// egui zeichnet ein Tab immer gleich breit. Damit Tabs bis zum nächsten
/// Tabstopp reichen, bekommt jede mögliche Restbreite eine eigene Kopie der
/// Monospace-Schrift; `highlight` wählt pro Tab die passende.
pub fn install_tab_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let Some(mono) = defs.families.get(&egui::FontFamily::Monospace).and_then(|f| f.first()).cloned() else {
        return;
    };
    let Some(data) = defs.font_data.get(&mono).cloned() else { return };
    for n in 1..=MAX_TAB {
        let mut font = (*data).clone();
        font.tweak.tab_size = n as f32;
        let name = format!("{mono}-tab{n}");
        defs.font_data.insert(name.clone(), std::sync::Arc::new(font));
        defs.families.insert(tab_family(n), vec![name]);
    }
    ctx.set_fonts(defs);
}

pub enum EditorAction {
    /// Übersetzt und LST geschrieben: in den Simulator laden.
    Loaded(PathBuf),
    Saved,
    /// Nachschlagefenster mit den Befehlen öffnen.
    ShowReference,
}

pub struct Editor {
    pub open: bool,
    pub path: Option<PathBuf>,
    pub text: String,
    saved: String,
    /// Text der letzten fehlerfreien Übersetzung, None: seit dem Laden nicht übersetzt.
    assembled: Option<String>,
    errors: Vec<AsmError>,
    message: String,
    scroll_to_line: Option<usize>,
    confirm_discard: Option<Pending>,
    text_id: egui::Id,
    /// Abstand der Tabstopps in Zeichen.
    pub tab_width: usize,
    dialog: Option<FileDialog<EditorDialog>>,
    /// Editor steht in einem eigenen Fenster.
    pub popped: bool,
}

fn window_id() -> egui::Id {
    egui::Id::unique("editor_window")
}

#[derive(Clone, Copy)]
enum Pending {
    New,
    Open,
}

/// Was nach dem Speichern unter einem neuen Namen passieren soll.
#[derive(Clone, Copy)]
enum AfterSave {
    Saved,
    Assemble,
    Run(Pending),
}

enum EditorDialog {
    SaveAs(AfterSave),
    Open,
}

/// Wurde `a` später geändert als `b`?
fn newer(a: &Path, b: &Path) -> bool {
    let time = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    matches!((time(a), time(b)), (Some(a), Some(b)) if a > b)
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            open: false,
            path: None,
            text: TEMPLATE.to_string(),
            saved: TEMPLATE.to_string(),
            assembled: None,
            errors: Vec::new(),
            message: String::new(),
            scroll_to_line: None,
            confirm_discard: None,
            text_id: egui::Id::unique("editor_text"),
            popped: false,
            tab_width: DEFAULT_TAB,
            dialog: None,
        }
    }
}

impl Editor {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// Passt das geladene Programm `lst` nicht mehr zu diesem Quelltext?
    /// Etwa nach Strg+S ohne Übersetzen oder wenn die Quelle neuer ist als das Listing.
    pub fn outdated(&self, lst: &Path) -> bool {
        let Some(src) = &self.path else { return false };
        if src.with_extension("") != lst.with_extension("") {
            return false;
        }
        match &self.assembled {
            Some(text) => *text != self.text,
            None => self.dirty() || newer(src, lst),
        }
    }

    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        self.open && ctx.memory(|m| m.has_focus(self.text_id))
    }

    pub fn load(&mut self, path: &Path) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let text = picsim_core::lst::decode_text(&bytes).replace("\r\n", "\n");
                self.text = text.clone();
                self.saved = text;
                self.assembled = None;
                self.path = Some(path.to_path_buf());
                self.errors.clear();
                self.message = format!("{} geöffnet", path.display());
            }
            Err(e) => self.message = format!("{} lässt sich nicht öffnen: {e}", path.display()),
        }
    }

    /// Quelltext so, wie er in die Datei geschrieben wird.
    pub fn file_bytes(&self) -> Vec<u8> {
        // Wie das Original: Latin-1 mit CRLF, damit alte Werkzeuge die Datei lesen
        let text = self.text.replace('\n', "\r\n");
        text.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
    }

    /// Ungespeicherte Änderungen in die vorhandene Datei schreiben.
    pub fn save_if_dirty(&mut self) -> bool {
        match self.path.clone() {
            Some(p) if self.dirty() => self.save_to(&p),
            _ => true,
        }
    }

    /// Unverändert seit dem Start, also nichts, was ins Projekt müsste.
    pub fn is_template(&self) -> bool {
        self.path.is_none() && self.text == TEMPLATE
    }

    fn save_to(&mut self, path: &Path) -> bool {
        match std::fs::write(path, self.file_bytes()) {
            Ok(()) => {
                self.saved = self.text.clone();
                self.path = Some(path.to_path_buf());
                self.message = format!("{} gespeichert", path.display());
                true
            }
            Err(e) => {
                self.message = format!("Speichern fehlgeschlagen: {e}");
                false
            }
        }
    }

    /// Speichert sofort, wenn die Datei schon einen Namen hat. Sonst wird
    /// nach dem Namen gefragt und `after` folgt, sobald gespeichert ist.
    fn save(&mut self, after: AfterSave) -> bool {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => {
                self.save_as(after);
                false
            }
        }
    }

    fn save_as(&mut self, after: AfterSave) {
        if self.dialog.is_some() {
            return;
        }
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "programm.src".into());
        let dlg = rfd::AsyncFileDialog::new().add_filter("Quelltext", &["src", "SRC", "asm"]).set_file_name(name);
        self.dialog = Some(FileDialog::save(dlg, EditorDialog::SaveAs(after)));
    }

    fn open_dialog(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let dlg = rfd::AsyncFileDialog::new().add_filter("Quelltext", &["src", "SRC", "asm"]);
        self.dialog = Some(FileDialog::open(dlg, EditorDialog::Open));
    }

    /// Ergebnis eines fertigen Dateidialogs verarbeiten.
    fn finish_dialog(&mut self, ctx: &egui::Context) -> Option<EditorAction> {
        let (purpose, path) = dialog::poll(&mut self.dialog, ctx)?;
        let mut path = path?;
        match purpose {
            EditorDialog::Open => {
                self.load(&path);
                None
            }
            EditorDialog::SaveAs(after) => {
                if path.extension().is_none() {
                    path.set_extension("src");
                }
                if !self.save_to(&path) {
                    return None;
                }
                match after {
                    AfterSave::Saved => Some(EditorAction::Saved),
                    AfterSave::Assemble => self.assemble(),
                    AfterSave::Run(p) => {
                        self.run_pending(p);
                        None
                    }
                }
            }
        }
    }

    fn new_file(&mut self) {
        self.text = TEMPLATE.to_string();
        self.saved = String::new();
        self.assembled = None;
        self.path = None;
        self.errors.clear();
        self.message = "Neue Datei".into();
    }

    fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Speichert, übersetzt und schreibt LST/HEX/ERR/SYM neben die Quelle.
    pub fn assemble(&mut self) -> Option<EditorAction> {
        if (self.dirty() || self.path.is_none()) && !self.save(AfterSave::Assemble) {
            return None;
        }
        let path = self.path.clone()?;
        match picsim_asm::assemble_source_to(&self.text, &path) {
            Ok((out, lst)) => {
                self.errors = out.errors.clone();
                let n_err = out.errors.iter().filter(|e| !e.warning).count();
                if out.ok() {
                    self.assembled = Some(self.text.clone());
                    self.message = format!("Fehlerfrei übersetzt, {} von 1024 Wörtern belegt. Programm geladen.", out.words.len());
                    Some(EditorAction::Loaded(lst))
                } else {
                    self.message = format!("{n_err} Fehler, Programm wurde nicht geladen. Im Simulator läuft noch das alte.");
                    let file = self.file_name();
                    if let Some(first) = self.errors.iter().find(|e| !e.warning && e.file == file) {
                        self.scroll_to_line = Some(first.src_line);
                    }
                    None
                }
            }
            Err(e) => {
                self.message = format!("Ausgabedateien lassen sich nicht schreiben: {e}");
                None
            }
        }
    }

    fn title(&self) -> String {
        let name = self.file_name();
        let name = if name.is_empty() { "unbenannt.src".to_string() } else { name };
        format!("Quelltext: {name}{}", if self.dirty() { " *" } else { "" })
    }

    fn error_lines(&self) -> HashSet<usize> {
        let file = self.file_name();
        self.errors.iter().filter(|e| e.file == file).map(|e| e.src_line).collect()
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<EditorAction> {
        let mut action = self.finish_dialog(ctx);
        if !self.open {
            return action;
        }

        if self.popped {
            let title = self.title();
            let still = crate::popout::show(ctx, "editor", &title, egui::vec2(780.0, 640.0), |ui| {
                // Tastenkürzel und Rückfrage gehören hier ins eigene Fenster
                let vctx = ui.ctx().clone();
                if let Some(a) = self.keys(&vctx) {
                    action = Some(a);
                }
                if let Some(a) = self.body(ui) {
                    action = Some(a);
                }
                self.confirm_dialog(&vctx);
            });
            if !still {
                self.popped = false;
            }
            return action;
        }

        if let Some(a) = self.keys(ctx) {
            action = Some(a);
        }
        let mut open = true;
        egui::Window::new(self.title()).order(egui::Order::Foreground)
            .id(window_id())
            .open(&mut open)
            .default_size([780.0, 640.0])
            .default_pos([460.0, 60.0])
            .min_size([420.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                if crate::popout::title_button(ui, window_id(), true) {
                    self.popped = true;
                }
                if let Some(a) = self.body(ui) {
                    action = Some(a);
                }
            });
        if !open {
            self.open = false;
        }
        self.confirm_dialog(ctx);
        action
    }

    /// Tastenkürzel, solange der Editor offen ist.
    fn keys(&mut self, ctx: &egui::Context) -> Option<EditorAction> {
        let mut action = None;
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) && self.save(AfterSave::Saved) {
            action = Some(EditorAction::Saved);
        }
        let assemble_key = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::B) || i.consume_key(egui::Modifiers::NONE, egui::Key::F5)
        });
        if assemble_key {
            action = self.assemble();
        }
        action
    }

    /// Inhalt des Editorfensters.
    fn body(&mut self, ui: &mut egui::Ui) -> Option<EditorAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Neu").clicked() {
                self.ask_or_run(Pending::New);
            }
            if ui.button("Öffnen…").clicked() {
                self.ask_or_run(Pending::Open);
            }
            if ui.button("Speichern").on_hover_text("Strg+S").clicked() && self.save(AfterSave::Saved) {
                action = Some(EditorAction::Saved);
            }
            if ui.button("Speichern unter…").clicked() {
                self.save_as(AfterSave::Saved);
            }
            ui.separator();
            let asm_btn = egui::Button::new(RichText::new("Assemblieren & Laden").strong());
            if ui.add(asm_btn).on_hover_text("F5 oder Strg+B").clicked() {
                action = self.assemble();
            }
            if ui.button("Befehle").on_hover_text("Befehlsübersicht und Registertabelle").clicked() {
                action = Some(EditorAction::ShowReference);
            }
            ui.separator();
            ui.label("Tabbreite");
            ui.add(egui::DragValue::new(&mut self.tab_width).range(2..=MAX_TAB))
                .on_hover_text("Abstand der Tabstopps in Zeichen");
        });
        if !self.message.is_empty() {
            let ok = self.errors.iter().all(|e| e.warning);
            let col = if ok {
                crate::theme::pick(ui, Color32::from_rgb(20, 110, 40), Color32::from_rgb(110, 210, 120))
            } else {
                crate::theme::pick(ui, Color32::from_rgb(180, 20, 20), Color32::from_rgb(255, 110, 100))
            };
            ui.label(RichText::new(&self.message).color(col));
        }
        ui.separator();

        let err_h = if self.errors.is_empty() { 0.0 } else { 110.0 };
        let editor_h = (ui.available_height() - err_h).max(120.0);
        self.code_area(ui, editor_h);

        if !self.errors.is_empty() {
            ui.separator();
            let mut jump = None;
            egui::ScrollArea::vertical().id_salt("errors").max_height(err_h - 10.0).show(ui, |ui| {
                for e in &self.errors {
                    let kind = if e.warning { "Warnung" } else { "Fehler" };
                    let col = if e.warning {
                        crate::theme::pick(ui, Color32::from_rgb(160, 110, 0), Color32::from_rgb(235, 185, 70))
                    } else {
                        crate::theme::pick(ui, Color32::from_rgb(180, 20, 20), Color32::from_rgb(255, 110, 100))
                    };
                    let text = format!("{kind} in Zeile {} ({}): {}", e.src_line, e.file, e.msg);
                    let label = egui::Label::new(RichText::new(text).color(col)).sense(egui::Sense::click());
                    if ui.add(label).on_hover_text("Klicken springt zur Zeile").clicked() {
                        jump = Some(e.src_line);
                    }
                }
            });
            if jump.is_some() {
                self.scroll_to_line = jump;
            }
        }
        action
    }

    fn code_area(&mut self, ui: &mut egui::Ui, height: f32) {
        let font = FontId::monospace(14.0);
        let row_h = ui.ctx().fonts_mut(|f| f.row_height(&font));
        let errors = self.error_lines();
        let tab = self.tab_width.clamp(2, MAX_TAB);

        let mut area = egui::ScrollArea::both().id_salt("code").max_height(height).auto_shrink([false, false]);
        if let Some(line) = self.scroll_to_line.take() {
            area = area.vertical_scroll_offset(((line as f32 - 5.0) * row_h).max(0.0));
            self.place_cursor(ui.ctx(), line);
        }
        area.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let lines = self.text.split('\n').count();
                let numbers: String = (1..=lines).map(|n| format!("{n:>4}\n")).collect();
                let gutter = RichText::new(numbers.trim_end()).font(font.clone()).color(Color32::GRAY);
                ui.add(egui::Label::new(gutter).selectable(false));

                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                    let pal = if ui.visuals().dark_mode { &DARK } else { &LIGHT };
                    let job = highlight(buf.as_str(), &errors, &font, tab, pal);
                    ui.ctx().fonts_mut(|f| f.layout_job(job))
                };
                ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .id(self.text_id)
                        .code_editor()
                        .font(font.clone())
                        .desired_width(f32::INFINITY)
                        .desired_rows(30)
                        .margin(egui::Margin::symmetric(4, 0))
                        .frame(egui::Frame::NONE)
                        .layouter(&mut layouter),
                );
            });
        });
    }

    /// Fügt Text an der Cursorposition ein (ersetzt eine Auswahl) und öffnet
    /// den Editor. `instruction`: Am Zeilenanfang wird ein Tab davorgesetzt,
    /// damit der Befehl nicht als Label gilt.
    pub fn insert(&mut self, ctx: &egui::Context, text: &str, instruction: bool) {
        let total = self.text.chars().count();
        let (start, end) = egui::TextEdit::load_state(ctx, self.text_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let (a, b) = (r.primary.index.0, r.secondary.index.0);
                (a.min(b).min(total), a.max(b).min(total))
            })
            .unwrap_or((total, total));
        let byte = |ci: usize| self.text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(self.text.len());
        let (b0, b1) = (byte(start), byte(end));
        let line_start = b0 == 0 || self.text[..b0].ends_with('\n');
        let ins = if instruction && line_start { format!("\t{text}") } else { text.to_string() };
        self.text.replace_range(b0..b1, &ins);

        let cursor = start + ins.chars().count();
        let mut state = egui::TextEdit::load_state(ctx, self.text_id).unwrap_or_default();
        let c = egui::text::CCursor::new(cursor);
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(c)));
        state.store(ctx, self.text_id);
        ctx.memory_mut(|m| m.request_focus(self.text_id));
        self.open = true;
    }

    fn place_cursor(&self, ctx: &egui::Context, line: usize) {
        let idx: usize = self.text.split('\n').take(line.saturating_sub(1)).map(|l| l.chars().count() + 1).sum();
        if let Some(mut state) = egui::TextEdit::load_state(ctx, self.text_id) {
            let c = egui::text::CCursor::new(idx);
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(c)));
            state.store(ctx, self.text_id);
        }
        ctx.memory_mut(|m| m.request_focus(self.text_id));
    }

    fn ask_or_run(&mut self, p: Pending) {
        if self.dirty() {
            self.confirm_discard = Some(p);
        } else {
            self.run_pending(p);
        }
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.confirm_discard else { return };
        egui::Window::new("Änderungen verwerfen?").order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Der Quelltext hat ungespeicherte Änderungen.");
                ui.horizontal(|ui| {
                    if ui.button("Speichern").clicked() {
                        self.confirm_discard = None;
                        if self.save(AfterSave::Run(pending)) {
                            self.run_pending(pending);
                        }
                    }
                    if ui.button("Verwerfen").clicked() {
                        self.confirm_discard = None;
                        self.run_pending(pending);
                    }
                    if ui.button("Abbrechen").clicked() {
                        self.confirm_discard = None;
                    }
                });
            });
    }

    fn run_pending(&mut self, p: Pending) {
        match p {
            Pending::New => self.new_file(),
            Pending::Open => self.open_dialog(),
        }
    }
}

// ----------------------------------------------------------------------
// Syntax-Hervorhebung

struct Palette {
    comment: Color32,
    label: Color32,
    mnemonic: Color32,
    directive: Color32,
    number: Color32,
    text: Color32,
    error_bg: Color32,
}

const LIGHT: Palette = Palette {
    comment: Color32::from_rgb(100, 130, 100),
    label: Color32::from_rgb(150, 40, 40),
    mnemonic: Color32::from_rgb(20, 60, 180),
    directive: Color32::from_rgb(130, 40, 150),
    number: Color32::from_rgb(190, 100, 0),
    text: Color32::from_rgb(30, 30, 30),
    error_bg: Color32::from_rgb(255, 215, 215),
};

const DARK: Palette = Palette {
    comment: Color32::from_rgb(120, 165, 120),
    label: Color32::from_rgb(235, 125, 110),
    mnemonic: Color32::from_rgb(110, 165, 255),
    directive: Color32::from_rgb(205, 135, 235),
    number: Color32::from_rgb(240, 170, 80),
    text: Color32::from_rgb(220, 220, 220),
    error_bg: Color32::from_rgb(95, 35, 35),
};

const PUNCT: &str = ",()+-";

/// Hängt `text` an und lässt jedes Tab bis zum nächsten Tabstopp reichen.
/// `col` ist die Spalte in der aktuellen Zeile.
fn append(job: &mut LayoutJob, text: &str, fmt: &TextFormat, col: &mut usize, tab: usize) {
    for (i, part) in text.split('\t').enumerate() {
        if i > 0 {
            let n = tab - *col % tab;
            let mut f = fmt.clone();
            f.font_id.family = tab_family(n);
            job.append("\t", 0.0, f);
            *col += n;
        }
        if !part.is_empty() {
            job.append(part, 0.0, fmt.clone());
            *col += part.chars().count();
        }
    }
}

fn highlight(text: &str, error_lines: &HashSet<usize>, font: &FontId, tab: usize, pal: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let bg = if error_lines.contains(&(i + 1)) { pal.error_bg } else { Color32::TRANSPARENT };
        let fmt = |color: Color32| TextFormat { font_id: font.clone(), color, background: bg, ..Default::default() };
        let (code, comment) = match line.find(';') {
            Some(p) => line.split_at(p),
            None => (line, ""),
        };
        let mut col = 0;
        let mut first = true;
        let mut rest = code;
        while let Some(c) = rest.chars().next() {
            let len = if c.is_whitespace() || PUNCT.contains(c) {
                c.len_utf8()
            } else {
                rest.find(|ch: char| ch.is_whitespace() || PUNCT.contains(ch)).unwrap_or(rest.len())
            };
            let (tok, tail) = rest.split_at(len);
            let word = !c.is_whitespace() && !PUNCT.contains(c);
            let color = if !word {
                pal.text
            } else if first && line.starts_with(tok) {
                pal.label
            } else if picsim_asm::is_mnemonic(tok) {
                pal.mnemonic
            } else if picsim_asm::is_directive(tok) {
                pal.directive
            } else if tok.starts_with(|c: char| c.is_ascii_digit() || c == '\'' || c == '$') {
                pal.number
            } else {
                pal.text
            };
            first &= !word;
            append(&mut job, tok, &fmt(color), &mut col, tab);
            rest = tail;
        }
        if !comment.is_empty() {
            append(&mut job, comment, &fmt(pal.comment), &mut col, tab);
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_keeps_text() {
        let src = "loop\tbsf STATUS,5 ; Kommentar\n\tgoto loop\n";
        let job = highlight(src, &HashSet::new(), &FontId::monospace(14.0), 8, &LIGHT);
        assert_eq!(job.text, src);
        let colors: Vec<_> = job.sections.iter().map(|s| (&job.text[s.byte_range.start.0..s.byte_range.end.0], s.format.color)).collect();
        assert!(colors.contains(&("loop", LIGHT.label)));
        assert!(colors.contains(&("bsf", LIGHT.mnemonic)));
        assert!(colors.contains(&("5", LIGHT.number)));
        assert!(colors.contains(&("goto", LIGHT.mnemonic)));
        assert!(colors.contains(&("; Kommentar\n", LIGHT.comment)));
    }

    #[test]
    fn tabs_reach_next_stop() {
        // "status" und "porta" sind verschieden lang, "equ" steht trotzdem in Spalte 8
        let job = highlight("status\tequ\t3\nporta\tequ\t5\n\t; a\tb\n", &HashSet::new(), &FontId::monospace(14.0), 8, &LIGHT);
        let tabs: Vec<_> = job
            .sections
            .iter()
            .filter(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0] == "\t")
            .map(|s| s.format.font_id.family.clone())
            .collect();
        assert_eq!(tabs, [tab_family(2), tab_family(5), tab_family(3), tab_family(5), tab_family(8), tab_family(5)]);
    }
}
