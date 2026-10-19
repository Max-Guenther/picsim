//! Nachschlagefenster: Befehlssatz (Datenblatt Tabelle 9-2) und
//! Registerübersicht (Datenblatt Tabelle 4-1) des PIC16F84.

use eframe::egui::{self, Color32, RichText};
use picsim_core::Cpu;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Befehle,
    Register,
}

#[derive(Default)]
pub struct Reference {
    pub open: bool,
    pub tab: Tab,
    filter: String,
}

/// Mnemonic, Operanden, Beschreibung, Zyklen, Opcode, Flags, Hinweise
type Instr = (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str, &'static str);

const BYTE_OPS: &[Instr] = &[
    ("ADDWF", "f, d", "W und f addieren", "1", "00 0111 dfff ffff", "C, DC, Z", "1, 2"),
    ("ANDWF", "f, d", "W UND f", "1", "00 0101 dfff ffff", "Z", "1, 2"),
    ("CLRF", "f", "f löschen", "1", "00 0001 1fff ffff", "Z", "2"),
    ("CLRW", "-", "W löschen", "1", "00 0001 0xxx xxxx", "Z", ""),
    ("COMF", "f, d", "f invertieren (Einerkomplement)", "1", "00 1001 dfff ffff", "Z", "1, 2"),
    ("DECF", "f, d", "f um 1 verringern", "1", "00 0011 dfff ffff", "Z", "1, 2"),
    ("DECFSZ", "f, d", "f um 1 verringern, überspringen wenn 0", "1 (2)", "00 1011 dfff ffff", "", "1, 2, 3"),
    ("INCF", "f, d", "f um 1 erhöhen", "1", "00 1010 dfff ffff", "Z", "1, 2"),
    ("INCFSZ", "f, d", "f um 1 erhöhen, überspringen wenn 0", "1 (2)", "00 1111 dfff ffff", "", "1, 2, 3"),
    ("IORWF", "f, d", "W ODER f", "1", "00 0100 dfff ffff", "Z", "1, 2"),
    ("MOVF", "f, d", "f kopieren (nach W oder f)", "1", "00 1000 dfff ffff", "Z", "1, 2"),
    ("MOVWF", "f", "W nach f kopieren", "1", "00 0000 1fff ffff", "", ""),
    ("NOP", "-", "Keine Operation", "1", "00 0000 0xx0 0000", "", ""),
    ("RLF", "f, d", "f über Carry links rotieren", "1", "00 1101 dfff ffff", "C", "1, 2"),
    ("RRF", "f, d", "f über Carry rechts rotieren", "1", "00 1100 dfff ffff", "C", "1, 2"),
    ("SUBWF", "f, d", "W von f subtrahieren (f - W)", "1", "00 0010 dfff ffff", "C, DC, Z", "1, 2"),
    ("SWAPF", "f, d", "Nibbles in f tauschen", "1", "00 1110 dfff ffff", "", "1, 2"),
    ("XORWF", "f, d", "W EXKLUSIV-ODER f", "1", "00 0110 dfff ffff", "Z", "1, 2"),
];

const BIT_OPS: &[Instr] = &[
    ("BCF", "f, b", "Bit b in f löschen", "1", "01 00bb bfff ffff", "", "1, 2"),
    ("BSF", "f, b", "Bit b in f setzen", "1", "01 01bb bfff ffff", "", "1, 2"),
    ("BTFSC", "f, b", "Bit b in f testen, überspringen wenn 0", "1 (2)", "01 10bb bfff ffff", "", "3"),
    ("BTFSS", "f, b", "Bit b in f testen, überspringen wenn 1", "1 (2)", "01 11bb bfff ffff", "", "3"),
];

const LIT_OPS: &[Instr] = &[
    ("ADDLW", "k", "Konstante und W addieren", "1", "11 111x kkkk kkkk", "C, DC, Z", ""),
    ("ANDLW", "k", "Konstante UND W", "1", "11 1001 kkkk kkkk", "Z", ""),
    ("CALL", "k", "Unterprogramm aufrufen", "2", "10 0kkk kkkk kkkk", "", ""),
    ("CLRWDT", "-", "Watchdog zurücksetzen", "1", "00 0000 0110 0100", "TO, PD", ""),
    ("GOTO", "k", "Sprung zu Adresse k", "2", "10 1kkk kkkk kkkk", "", ""),
    ("IORLW", "k", "Konstante ODER W", "1", "11 1000 kkkk kkkk", "Z", ""),
    ("MOVLW", "k", "Konstante nach W laden", "1", "11 00xx kkkk kkkk", "", ""),
    ("RETFIE", "-", "Rücksprung aus Interrupt (setzt GIE)", "2", "00 0000 0000 1001", "", ""),
    ("RETLW", "k", "Rücksprung mit Konstante in W", "2", "11 01xx kkkk kkkk", "", ""),
    ("RETURN", "-", "Rücksprung aus Unterprogramm", "2", "00 0000 0000 1000", "", ""),
    ("SLEEP", "-", "Stromsparmodus", "1", "00 0000 0110 0011", "TO, PD", ""),
    ("SUBLW", "k", "W von Konstante subtrahieren (k - W)", "1", "11 110x kkkk kkkk", "C, DC, Z", ""),
    ("XORLW", "k", "Konstante EXKLUSIV-ODER W", "1", "11 1010 kkkk kkkk", "Z", ""),
];

const NOTES: &[&str] = &[
    "f = Registeradresse (0..7F), d = Ziel: 0 bzw. W = nach W, 1 bzw. F = nach f (ohne Angabe: f), b = Bitnummer 0..7, k = Konstante bzw. Adresse.",
    "Hinweis 1: Wird ein Port mit sich selbst verknüpft (z.B. MOVF PORTB,1), gilt der Pegel an den Pins, nicht das Latch.",
    "Hinweis 2: Schreibt der Befehl auf TMR0, wird ein zugeordneter Vorteiler gelöscht.",
    "Hinweis 3: Wird der PC verändert oder ist die Sprungbedingung erfüllt, dauert der Befehl 2 Zyklen (der zweite ist ein NOP).",
];

/// Pseudobefehl, Bedeutung, wird übersetzt zu
const PSEUDO: &[(&str, &str, &str)] = &[
    ("CLRC / SETC", "Carry löschen / setzen", "BCF / BSF STATUS,C"),
    ("CLRZ / SETZ", "Zero-Flag löschen / setzen", "BCF / BSF STATUS,Z"),
    ("CLRDC / SETDC", "Digit Carry löschen / setzen", "BCF / BSF STATUS,DC"),
    ("SKPC / SKPNC", "Überspringen wenn C = 1 / C = 0", "BTFSS / BTFSC STATUS,C"),
    ("SKPZ / SKPNZ", "Überspringen wenn Z = 1 / Z = 0", "BTFSS / BTFSC STATUS,Z"),
    ("SKPDC / SKPNDC", "Überspringen wenn DC = 1 / DC = 0", "BTFSS / BTFSC STATUS,DC"),
    ("B k", "Sprung", "GOTO k"),
    ("BC / BNC k", "Sprung wenn C = 1 / C = 0", "BTFSC / BTFSS STATUS,C + GOTO k"),
    ("BZ / BNZ k", "Sprung wenn Z = 1 / Z = 0", "BTFSC / BTFSS STATUS,Z + GOTO k"),
    ("BDC / BNDC k", "Sprung wenn DC = 1 / DC = 0", "BTFSC / BTFSS STATUS,DC + GOTO k"),
    ("MOVFW f", "f nach W", "MOVF f,0"),
    ("TSTF f", "f auf 0 prüfen (setzt Z)", "MOVF f,1"),
    ("NEGF f, d", "f negieren (Zweierkomplement)", "COMF f,1 + INCF f,d"),
    ("ADDCF / SUBCF f, d", "Carry zu f addieren / von f abziehen", "BTFSC STATUS,C + INCF / DECF f,d"),
    ("SETB / CLRB f, b", "Bit setzen / löschen", "BSF / BCF f,b"),
    ("SNB / SB f, b", "Überspringen wenn Bit 0 / 1", "BTFSC / BTFSS f,b"),
    ("LGOTO / LCALL k", "Sprung / Aufruf über Seitengrenzen", "PCLATH setzen + GOTO / CALL"),
];

/// Direktive, Bedeutung, Beispiel
const DIRECTIVES: &[(&str, &str, &str)] = &[
    ("EQU", "Symbol festlegen (auch Register,Bit)", "zaehler EQU 20h   taste EQU 6,4"),
    ("SET / =", "Symbol festlegen, darf sich ändern", "n SET n+1"),
    ("ORG", "Programmadresse setzen", "ORG 0"),
    ("END", "Ende des Quelltexts", "END"),
    ("DEVICE", "PIC-Typ", "DEVICE 16F84"),
    ("CONFIG", "Konfigurationswort", "CONFIG WDT_OFF, XT_OSC"),
    ("INCLUDE", "Andere Datei einfügen", "INCLUDE \"defs.inc\""),
    ("IF / ELSEIF / ELSE / ENDIF", "Bedingt übersetzen", "IF zaehler = 20h"),
    ("IFDEF / IFNDEF", "Übersetzen, wenn Symbol (nicht) definiert", "IFDEF DEBUG"),
    ("DEFINE", "Symbol festlegen", "DEFINE DEBUG"),
    ("DT / RETW", "Tabelle aus RETLW-Befehlen, auch Text", "DT \"Hallo\",0"),
    ("DW / DATA", "Programmwörter direkt", "DW 3FFFh"),
];

const NUMBERS: &str = "Zahlen: 35 (dezimal), 0FFH oder 0x1F oder $1F (hex), 01B oder 0b101 (binär), 'A' (Zeichen). \
Ausdrücke mit + - * / % & | ^ ~ << >> und Klammern, $ ist die aktuelle Adresse, HIGH() und LOW() liefern die Bytes.";

#[derive(Clone, Copy)]
enum RegBits {
    Bits([&'static str; 8]),
    Text(&'static str),
}

/// Adresse, Name, Bits 7..0 (oder ein Text über alle Bits), Power-on-Reset, andere Resets
type RegRow = (&'static str, &'static str, RegBits, &'static str, &'static str);

const STATUS_BITS: RegBits = RegBits::Bits(["IRP", "RP1", "RP0", "TO", "PD", "Z", "DC", "C"]);
const INTCON_BITS: RegBits = RegBits::Bits(["GIE", "EEIE", "T0IE", "INTE", "RBIE", "T0IF", "INTF", "RBIF"]);
const INDF_TEXT: RegBits = RegBits::Text("Nutzt den Inhalt von FSR als Adresse (kein echtes Register)");
const PCL_TEXT: RegBits = RegBits::Text("Untere 8 Bits des Programmzählers (PC)");
const FSR_TEXT: RegBits = RegBits::Text("Zeiger für indirekte Adressierung");
const PCLATH_TEXT: RegBits = RegBits::Text("— — — Puffer für die oberen 5 Bits des PC");
const FREE_TEXT: RegBits = RegBits::Text("Nicht belegt, liest 0");

const REGISTERS_BANK0: &[RegRow] = &[
    ("00h", "INDF", INDF_TEXT, "---- ----", "---- ----"),
    ("01h", "TMR0", RegBits::Text("8-Bit-Zähler/Timer"), "xxxx xxxx", "uuuu uuuu"),
    ("02h", "PCL", PCL_TEXT, "0000 0000", "0000 0000"),
    ("03h", "STATUS", STATUS_BITS, "0001 1xxx", "000q quuu"),
    ("04h", "FSR", FSR_TEXT, "xxxx xxxx", "uuuu uuuu"),
    ("05h", "PORTA", RegBits::Bits(["—", "—", "—", "RA4/T0CKI", "RA3", "RA2", "RA1", "RA0"]), "---x xxxx", "---u uuuu"),
    ("06h", "PORTB", RegBits::Bits(["RB7", "RB6", "RB5", "RB4", "RB3", "RB2", "RB1", "RB0/INT"]), "xxxx xxxx", "uuuu uuuu"),
    ("07h", "", FREE_TEXT, "---- ----", "---- ----"),
    ("08h", "EEDATA", RegBits::Text("EEPROM-Datenregister"), "xxxx xxxx", "uuuu uuuu"),
    ("09h", "EEADR", RegBits::Text("EEPROM-Adressregister"), "xxxx xxxx", "uuuu uuuu"),
    ("0Ah", "PCLATH", PCLATH_TEXT, "---0 0000", "---0 0000"),
    ("0Bh", "INTCON", INTCON_BITS, "0000 000x", "0000 000u"),
];

const REGISTERS_BANK1: &[RegRow] = &[
    ("80h", "INDF", INDF_TEXT, "---- ----", "---- ----"),
    ("81h", "OPTION_REG", RegBits::Bits(["RBPU", "INTEDG", "T0CS", "T0SE", "PSA", "PS2", "PS1", "PS0"]), "1111 1111", "1111 1111"),
    ("82h", "PCL", PCL_TEXT, "0000 0000", "0000 0000"),
    ("83h", "STATUS", STATUS_BITS, "0001 1xxx", "000q quuu"),
    ("84h", "FSR", FSR_TEXT, "xxxx xxxx", "uuuu uuuu"),
    ("85h", "TRISA", RegBits::Text("— — — Richtung Port A (1 = Eingang)"), "---1 1111", "---1 1111"),
    ("86h", "TRISB", RegBits::Text("Richtung Port B (1 = Eingang)"), "1111 1111", "1111 1111"),
    ("87h", "", FREE_TEXT, "---- ----", "---- ----"),
    ("88h", "EECON1", RegBits::Bits(["—", "—", "—", "EEIF", "WRERR", "WREN", "WR", "RD"]), "---0 x000", "---0 q000"),
    ("89h", "EECON2", RegBits::Text("EEPROM-Steuerregister 2 (kein echtes Register)"), "---- ----", "---- ----"),
    ("8Ah", "PCLATH", PCLATH_TEXT, "---0 0000", "---0 0000"),
    ("8Bh", "INTCON", INTCON_BITS, "0000 000x", "0000 000u"),
];

const REG_NOTES: &[&str] = &[
    "Legende: x = unbekannt, u = unverändert, - = nicht belegt (liest 0), q = hängt von der Ursache ab.",
    "Andere Resets: externer Reset über MCLR und Watchdog-Reset. TO und PD ändern sich bei MCLR nicht.",
    "Der obere Teil des PC ist nicht direkt lesbar. PCLATH wird beim Schreiben von PCL bzw. bei GOTO und CALL in den PC übernommen.",
    "0Ch bis 4Fh: 68 freie Register (SRAM), in Bank 1 gespiegelt (8Ch bis CFh).",
];

/// Was ein Klick in den Editor einfügen soll: Text und ob es ein Befehl ist
/// (dann am Zeilenanfang mit Tab davor).
pub type Insert = (String, bool);

const HINT: &str = "Klicken: an der Cursorposition im Editor einfügen";
const BLUE: Color32 = Color32::from_rgb(20, 60, 180);
const PURPLE: Color32 = Color32::from_rgb(130, 40, 150);
const GROUP: Color32 = Color32::from_rgb(40, 60, 140);

/// Anklickbarer Text. Beim Überfahren Rahmen und Hinweis, beim Klick Einfügen.
fn clickable(ui: &mut egui::Ui, text: RichText, insert: Option<Insert>, out: &mut Option<Insert>) -> egui::Response {
    let Some(ins) = insert else {
        return ui.label(text);
    };
    let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    clickable_response(ui, r, ins, out)
}

fn clickable_response(ui: &egui::Ui, r: egui::Response, ins: Insert, out: &mut Option<Insert>) -> egui::Response {
    if r.hovered() {
        ui.painter().rect_stroke(r.rect.expand(2.0), 3.0, (1.0, BLUE), egui::StrokeKind::Outside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.clicked() {
        *out = Some(ins.clone());
    }
    r.on_hover_text(format!("{HINT}: {}", ins.0.trim()))
}

/// Erstes Wort eines Tabelleneintrags ("BC / BNC k" -> "BC", "BNC").
fn words(entry: &str) -> Vec<&str> {
    entry.split(" / ").map(|p| p.split_whitespace().next().unwrap_or(p)).collect()
}

impl Reference {
    /// Zeigt das Fenster. Liefert den Text, der in den Editor soll.
    pub fn show(&mut self, ctx: &egui::Context, cpu: &Cpu) -> Option<Insert> {
        let mut out = None;
        let mut open = self.open;
        egui::Window::new("Nachschlagen")
            .order(egui::Order::Foreground)
            .open(&mut open)
            .resizable(true)
            .default_size([1330.0, 700.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Befehle, RichText::new("Befehle").size(16.0));
                    ui.selectable_value(&mut self.tab, Tab::Register, RichText::new("Register (Tabelle 4-1)").size(16.0));
                    if self.tab == Tab::Befehle {
                        ui.separator();
                        let search = egui::TextEdit::singleline(&mut self.filter).hint_text("Suchen…").desired_width(180.0);
                        ui.add(search);
                    }
                });
                ui.label(RichText::new("Einträge anklicken, um sie im Editor an der Cursorposition einzufügen.").weak());
                ui.separator();
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Befehle => self.instructions(ui, &mut out),
                    Tab::Register => registers(ui, cpu, &mut out),
                });
            });
        self.open = open;
        out
    }

    fn matches(&self, texts: &[&str]) -> bool {
        let f = self.filter.trim().to_lowercase();
        f.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(&f))
    }

    fn instructions(&self, ui: &mut egui::Ui, out: &mut Option<Insert>) {
        ui.label(RichText::new("Befehlssatz PIC16F84 (Datenblatt Tabelle 9-2)").strong().size(16.0));
        egui::Grid::new("instr").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
            for h in ["Befehl", "Operanden", "Beschreibung", "Zyklen", "14-Bit-Opcode", "Flags", "Hinweise"] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            let groups = [
                ("Byte-Befehle (Register)", BYTE_OPS),
                ("Bit-Befehle (Register)", BIT_OPS),
                ("Konstanten- und Sprungbefehle", LIT_OPS),
            ];
            for (title, rows) in groups {
                let shown: Vec<_> = rows.iter().filter(|r| self.matches(&[r.0, r.2, r.5])).collect();
                if shown.is_empty() {
                    continue;
                }
                ui.label(RichText::new(title).italics().color(GROUP));
                ui.end_row();
                for (m, ops, desc, cyc, code, flags, notes) in shown {
                    let ins = (format!("{}\t", m.to_lowercase()), true);
                    clickable(ui, RichText::new(*m).monospace().strong().color(BLUE), Some(ins), out);
                    ui.label(RichText::new(*ops).monospace());
                    ui.label(*desc);
                    ui.label(*cyc);
                    ui.label(RichText::new(*code).monospace());
                    ui.label(*flags);
                    ui.label(*notes);
                    ui.end_row();
                }
            }
        });
        ui.add_space(6.0);
        for n in NOTES {
            ui.label(RichText::new(*n).weak());
        }

        let pseudo: Vec<_> = PSEUDO.iter().filter(|r| self.matches(&[r.0, r.1, r.2])).collect();
        if !pseudo.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new("Pseudobefehle (werden in echte Befehle übersetzt)").strong().size(16.0));
            egui::Grid::new("pseudo").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                for (m, desc, real) in pseudo {
                    ui.horizontal(|ui| {
                        for (k, w) in words(m).into_iter().enumerate() {
                            if k > 0 {
                                ui.label("/");
                            }
                            let ins = (format!("{}\t", w.to_lowercase()), true);
                            clickable(ui, RichText::new(w).monospace().strong(), Some(ins), out);
                        }
                    });
                    ui.label(*desc);
                    ui.label(RichText::new(*real).monospace().weak());
                    ui.end_row();
                }
            });
        }

        let dirs: Vec<_> = DIRECTIVES.iter().filter(|r| self.matches(&[r.0, r.1])).collect();
        if !dirs.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new("Direktiven").strong().size(16.0));
            egui::Grid::new("dirs").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                for (d, desc, ex) in dirs {
                    ui.horizontal(|ui| {
                        for (k, w) in words(d).into_iter().enumerate() {
                            if k > 0 {
                                ui.label("/");
                            }
                            // EQU und SET stehen hinter einem Namen, die übrigen wie Befehle
                            let as_instr = !matches!(w, "EQU" | "SET" | "=");
                            let ins = (format!("{}\t", w.to_lowercase()), as_instr);
                            clickable(ui, RichText::new(w).monospace().strong().color(PURPLE), Some(ins), out);
                        }
                    });
                    ui.label(*desc);
                    ui.label(RichText::new(*ex).monospace().weak());
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(NUMBERS).weak());
        }
    }
}

/// Was ein Klick auf ein Bit einfügt, passend zu den vordefinierten Symbolen.
fn bit_insert(reg: &str, bit: usize, name: &str) -> Option<String> {
    if name == "—" {
        return None;
    }
    Some(match reg {
        "STATUS" => format!("STATUS,{name}"),
        "PORTA" => format!("PORTA,{bit}"),
        "PORTB" => format!("PORTB,{bit}"),
        "EECON1" if name == "WR" => "WR_".into(),
        "EECON1" if name == "RD" => "RD_".into(),
        _ => name.to_string(),
    })
}

fn registers(ui: &mut egui::Ui, cpu: &Cpu, out: &mut Option<Insert>) {
    ui.label(RichText::new("Register File Summary PIC16F84 (Datenblatt Tabelle 4-1, Seite 14)").strong().size(16.0));
    ui.label(RichText::new("Name anklicken fügt das Register ein, ein Bit fügt es so ein, wie der Assembler es erwartet (z.B. STATUS,RP0 oder GIE). Die Spalte \"Jetzt\" zeigt den aktuellen Wert.").weak());
    ui.add_space(6.0);

    // Feste Spaltenbreiten, damit Texte über alle acht Bitspalten gehen können
    let gap = 8.0;
    let widths = [70.0, 120.0, 92.0, 92.0, 92.0, 92.0, 92.0, 92.0, 92.0, 92.0, 104.0, 122.0, 104.0];
    let h = 28.0;
    let bits_w: f32 = widths[2..10].iter().sum::<f32>() + 7.0 * gap;
    let mono = |t: &str| RichText::new(t).monospace().size(14.0);

    // Eine Zelle mit fester Breite, optional anklickbar
    let cell = |ui: &mut egui::Ui, w: f32, text: RichText, insert: Option<String>, out: &mut Option<Insert>| {
        let sense = if insert.is_some() { egui::Sense::click() } else { egui::Sense::hover() };
        let r = ui.add_sized([w, h], egui::Label::new(text).sense(sense).truncate());
        if let Some(ins) = insert {
            clickable_response(ui, r, (ins, false), out);
        }
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let header = [
            "Adresse", "Name", "Bit 7", "Bit 6", "Bit 5", "Bit 4", "Bit 3", "Bit 2", "Bit 1", "Bit 0", "Power-on", "Andere Resets",
            "Jetzt",
        ];
        for (w, t) in widths.iter().zip(header) {
            cell(ui, *w, RichText::new(t).strong().size(14.0), None, out);
        }
    });
    ui.separator();
    for (title, rows) in [("Bank 0", REGISTERS_BANK0), ("Bank 1", REGISTERS_BANK1)] {
        ui.label(RichText::new(title).italics().strong().color(GROUP).size(15.0));
        for (k, (addr, name, bits, por, other)) in rows.iter().enumerate() {
            let fill = if k % 2 == 0 { ui.visuals().faint_bg_color } else { Color32::TRANSPARENT };
            egui::Frame::new().fill(fill).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    cell(ui, widths[0], mono(addr), Some(addr.to_string()), out);
                    let name_ins = (!name.is_empty()).then(|| name.to_string());
                    cell(ui, widths[1], mono(name).strong().color(BLUE), name_ins, out);
                    match bits {
                        RegBits::Bits(b) => {
                            for (j, (w, n)) in widths[2..10].iter().zip(b).enumerate() {
                                let ins = bit_insert(name, 7 - j, n);
                                cell(ui, *w, RichText::new(*n).size(14.0), ins, out);
                            }
                        }
                        RegBits::Text(t) => cell(ui, bits_w, RichText::new(*t).italics().size(14.0), None, out),
                    }
                    cell(ui, widths[10], mono(por), None, out);
                    cell(ui, widths[11], mono(other), None, out);
                    let a = u8::from_str_radix(addr.trim_end_matches('h'), 16).unwrap_or(0);
                    let real = Cpu::map(a).is_some() && !name.is_empty() && *name != "INDF" && *name != "EECON2";
                    let now = if real { format!("{:08b}", cpu.peek(a)) } else { String::new() };
                    cell(ui, widths[12], mono(&now).color(Color32::from_rgb(200, 40, 40)), None, out);
                });
            });
        }
        ui.add_space(4.0);
    }
    ui.add_space(6.0);
    for n in REG_NOTES {
        ui.label(RichText::new(*n).weak());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_inserts_match_assembler_symbols() {
        assert_eq!(bit_insert("STATUS", 5, "RP0").as_deref(), Some("STATUS,RP0"));
        assert_eq!(bit_insert("PORTB", 0, "RB0/INT").as_deref(), Some("PORTB,0"));
        assert_eq!(bit_insert("INTCON", 7, "GIE").as_deref(), Some("GIE"));
        assert_eq!(bit_insert("EECON1", 1, "WR").as_deref(), Some("WR_"));
        assert_eq!(bit_insert("PORTA", 7, "—"), None);
        // Alles, was eingefügt wird, muss der Assembler auch kennen
        for (_, name, bits, _, _) in REGISTERS_BANK0.iter().chain(REGISTERS_BANK1) {
            if let RegBits::Bits(b) = bits {
                for (j, n) in b.iter().enumerate() {
                    if let Some(ins) = bit_insert(name, 7 - j, n) {
                        let src = format!("\tbsf {ins}\n");
                        let o = picsim_asm::assemble(&src, "t.src", None);
                        assert!(o.ok(), "{ins}: {:?}", o.errors);
                    }
                }
            }
            if !name.is_empty() {
                let o = picsim_asm::assemble(&format!("\tmovf {name},w\n"), "t.src", None);
                assert!(o.ok(), "{name}: {:?}", o.errors);
            }
        }
    }

    #[test]
    fn words_split() {
        assert_eq!(words("BC / BNC k"), vec!["BC", "BNC"]);
        assert_eq!(words("MOVFW f"), vec!["MOVFW"]);
    }
}
