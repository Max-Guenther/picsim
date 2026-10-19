//! Eingebauter Editor für PIC-Assembler mit Syntax-Hervorhebung,
//! Zeilennummern, Fehleranzeige und "Assemblieren & Laden".

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
    errors: Vec<AsmError>,
    message: String,
    scroll_to_line: Option<usize>,
    confirm_discard: Option<Pending>,
    text_id: egui::Id,
}

#[derive(Clone, Copy)]
enum Pending {
    New,
    Open,
}

impl Default for Editor {
    fn default() -> Self {
        Editor {
            open: false,
            path: None,
            text: TEMPLATE.to_string(),
            saved: TEMPLATE.to_string(),
            errors: Vec::new(),
            message: String::new(),
            scroll_to_line: None,
            confirm_discard: None,
            text_id: egui::Id::unique("editor_text"),
        }
    }
}

impl Editor {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
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
                self.path = Some(path.to_path_buf());
                self.errors.clear();
                self.message = format!("{} geöffnet", path.display());
            }
            Err(e) => self.message = format!("{} lässt sich nicht öffnen: {e}", path.display()),
        }
    }

    fn save_to(&mut self, path: &Path) -> bool {
        // Wie das Original: Latin-1 mit CRLF, damit alte Werkzeuge die Datei lesen
        let text = self.text.replace('\n', "\r\n");
        let bytes: Vec<u8> = text.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
        match std::fs::write(path, bytes) {
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

    fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "programm.src".into());
        let dlg = rfd::FileDialog::new().add_filter("Quelltext", &["src", "SRC", "asm"]).set_file_name(name);
        match dlg.save_file() {
            Some(mut p) => {
                if p.extension().is_none() {
                    p.set_extension("src");
                }
                self.save_to(&p)
            }
            None => false,
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Quelltext", &["src", "SRC", "asm"]).pick_file() {
            self.load(&p);
        }
    }

    fn new_file(&mut self) {
        self.text = TEMPLATE.to_string();
        self.saved = String::new();
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
        if (self.dirty() || self.path.is_none()) && !self.save() {
            return None;
        }
        let path = self.path.clone()?;
        match picsim_asm::assemble_source_to(&self.text, &path) {
            Ok((out, lst)) => {
                self.errors = out.errors.clone();
                let n_err = out.errors.iter().filter(|e| !e.warning).count();
                if out.ok() {
                    self.message = format!("Fehlerfrei übersetzt, {} von 1024 Wörtern belegt. Programm geladen.", out.words.len());
                    Some(EditorAction::Loaded(lst))
                } else {
                    self.message = format!("{n_err} Fehler, Programm wurde nicht geladen");
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
        if !self.open {
            return None;
        }
        let mut action = None;

        // Tastenkürzel, solange der Editor offen ist
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) && self.save() {
            action = Some(EditorAction::Saved);
        }
        let assemble_key = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::B) || i.consume_key(egui::Modifiers::NONE, egui::Key::F5)
        });
        if assemble_key {
            action = self.assemble();
        }

        let mut open = true;
        egui::Window::new(self.title()).order(egui::Order::Foreground)
            .id(egui::Id::unique("editor_window"))
            .open(&mut open)
            .default_size([780.0, 640.0])
            .default_pos([460.0, 60.0])
            .min_size([420.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Neu").clicked() {
                        self.ask_or_run(Pending::New);
                    }
                    if ui.button("Öffnen…").clicked() {
                        self.ask_or_run(Pending::Open);
                    }
                    if ui.button("Speichern").on_hover_text("Strg+S").clicked() && self.save() {
                        action = Some(EditorAction::Saved);
                    }
                    if ui.button("Speichern unter…").clicked() && self.save_as() {
                        action = Some(EditorAction::Saved);
                    }
                    ui.separator();
                    let asm_btn = egui::Button::new(RichText::new("Assemblieren & Laden").strong());
                    if ui.add(asm_btn).on_hover_text("F5 oder Strg+B").clicked() {
                        action = self.assemble();
                    }
                    if ui.button("Befehle").on_hover_text("Befehlsübersicht und Registertabelle").clicked() {
                        action = Some(EditorAction::ShowReference);
                    }
                });
                if !self.message.is_empty() {
                    let ok = self.errors.iter().all(|e| e.warning);
                    let col = if ok { Color32::from_rgb(20, 110, 40) } else { Color32::from_rgb(180, 20, 20) };
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
                            let col = if e.warning { Color32::from_rgb(160, 110, 0) } else { Color32::from_rgb(180, 20, 20) };
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
            });
        if !open {
            self.open = false;
        }
        self.confirm_dialog(ctx);
        action
    }

    fn code_area(&mut self, ui: &mut egui::Ui, height: f32) {
        let font = FontId::monospace(14.0);
        let row_h = ui.ctx().fonts_mut(|f| f.row_height(&font));
        let errors = self.error_lines();

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
                    let job = highlight(buf.as_str(), &errors, &font);
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
                    if ui.button("Speichern").clicked() && self.save() {
                        self.confirm_discard = None;
                        self.run_pending(pending);
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

const COMMENT: Color32 = Color32::from_rgb(100, 130, 100);
const LABEL: Color32 = Color32::from_rgb(150, 40, 40);
const MNEMONIC: Color32 = Color32::from_rgb(20, 60, 180);
const DIRECTIVE: Color32 = Color32::from_rgb(130, 40, 150);
const NUMBER: Color32 = Color32::from_rgb(190, 100, 0);
const TEXT: Color32 = Color32::from_rgb(30, 30, 30);
const ERROR_BG: Color32 = Color32::from_rgb(255, 215, 215);

const PUNCT: &str = ",()+-";

fn highlight(text: &str, error_lines: &HashSet<usize>, font: &FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let bg = if error_lines.contains(&(i + 1)) { ERROR_BG } else { Color32::TRANSPARENT };
        let fmt = |color: Color32| TextFormat { font_id: font.clone(), color, background: bg, ..Default::default() };
        let (code, comment) = match line.find(';') {
            Some(p) => line.split_at(p),
            None => (line, ""),
        };
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
                TEXT
            } else if first && line.starts_with(tok) {
                LABEL
            } else if picsim_asm::is_mnemonic(tok) {
                MNEMONIC
            } else if picsim_asm::is_directive(tok) {
                DIRECTIVE
            } else if tok.starts_with(|c: char| c.is_ascii_digit() || c == '\'' || c == '$') {
                NUMBER
            } else {
                TEXT
            };
            first &= !word;
            job.append(tok, 0.0, fmt(color));
            rest = tail;
        }
        if !comment.is_empty() {
            job.append(comment, 0.0, fmt(COMMENT));
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
        let job = highlight(src, &HashSet::new(), &FontId::monospace(14.0));
        assert_eq!(job.text, src);
        let colors: Vec<_> = job.sections.iter().map(|s| (&job.text[s.byte_range.start.0..s.byte_range.end.0], s.format.color)).collect();
        assert!(colors.contains(&("loop", LABEL)));
        assert!(colors.contains(&("bsf", MNEMONIC)));
        assert!(colors.contains(&("5", NUMBER)));
        assert!(colors.contains(&("goto", MNEMONIC)));
        assert!(colors.contains(&("; Kommentar\n", COMMENT)));
    }
}
