//! Assembler für PIC16F84, kompatibel zu iL_ASS16.
//!
//! Erzeugt aus einer `.src`-Datei die Dateien `.LST` (für den Simulator),
//! `.HEX` (Intel-HEX für Programmiergeräte), `.ERR` und `.SYM`.

mod device;
pub mod expr;

use expr::{eval, ExprError, Symbols, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const PROG_WORDS: u16 = 1024;
pub const CONFIG_ADDR: u16 = 0x2007;

#[derive(Debug, Clone, PartialEq)]
pub struct AsmError {
    /// Zeilennummer im Listing (fortlaufend, 1-basiert).
    pub line: usize,
    pub file: String,
    /// Zeilennummer in der jeweiligen Quelldatei.
    pub src_line: usize,
    pub msg: String,
    pub warning: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Output {
    pub lst: String,
    pub hex: String,
    pub err: String,
    pub sym: String,
    pub words: Vec<(u16, u16)>,
    pub errors: Vec<AsmError>,
    pub config: u16,
    pub device: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        !self.errors.iter().any(|e| !e.warning)
    }
}

#[derive(Debug, Clone)]
struct SrcLine {
    text: String,
    file: String,
    lineno: usize,
}

/// Liest Quelltext inkl. INCLUDE-Dateien in eine flache Zeilenliste.
fn expand_includes(
    text: &str,
    file: &str,
    dir: Option<&Path>,
    depth: usize,
    out: &mut Vec<SrcLine>,
    errors: &mut Vec<(usize, String)>,
) {
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        out.push(SrcLine { text: line.to_string(), file: file.to_string(), lineno: i + 1 });
        let st = parse_statement(line);
        if !matches!(st.op.as_deref(), Some("INCLUDE") | Some("#INCLUDE")) {
            continue;
        }
        let name = st.operands.trim().trim_matches(|c| c == '"' || c == '<' || c == '>' || c == '\'');
        let path = match dir {
            Some(d) => d.join(name),
            None => PathBuf::from(name),
        };
        if depth > 8 {
            errors.push((out.len(), "Includedatei zu tief verschachtelt".into()));
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                let inc = decode(&bytes);
                let sub = path.parent().map(Path::to_path_buf);
                expand_includes(&inc, name, sub.as_deref(), depth + 1, out, errors);
            }
            Err(_) => errors.push((out.len(), format!("Includedatei nicht gefunden: {name}"))),
        }
    }
}

fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_quote = false;
    for (i, c) in line.char_indices() {
        match c {
            '\'' | '"' => in_quote = !in_quote,
            ';' if !in_quote => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Trennt an Kommas außerhalb von Klammern und Anführungszeichen.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut quote = false;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '\'' | '"' => {
                quote = !quote;
                cur.push(c)
            }
            '(' if !quote => {
                depth += 1;
                cur.push(c)
            }
            ')' if !quote => {
                depth -= 1;
                cur.push(c)
            }
            ',' if !quote && depth == 0 => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// f,d
    Fd,
    /// f
    F,
    /// keine Operanden
    Plain,
    /// f,b
    Fb,
    /// k (8 Bit)
    K8,
    /// k (11 Bit)
    K11,
}

fn opcode(name: &str) -> Option<(Kind, u16)> {
    use Kind::*;
    Some(match name {
        "ADDWF" => (Fd, 0x0700),
        "ANDWF" => (Fd, 0x0500),
        "CLRF" => (F, 0x0180),
        "CLRW" => (Plain, 0x0100),
        "COMF" => (Fd, 0x0900),
        "DECF" => (Fd, 0x0300),
        "DECFSZ" => (Fd, 0x0B00),
        "INCF" => (Fd, 0x0A00),
        "INCFSZ" => (Fd, 0x0F00),
        "IORWF" => (Fd, 0x0400),
        "MOVF" => (Fd, 0x0800),
        "MOVWF" => (F, 0x0080),
        "NOP" => (Plain, 0x0000),
        "RLF" => (Fd, 0x0D00),
        "RRF" => (Fd, 0x0C00),
        "SUBWF" => (Fd, 0x0200),
        "SWAPF" => (Fd, 0x0E00),
        "XORWF" => (Fd, 0x0600),
        "BCF" => (Fb, 0x1000),
        "BSF" => (Fb, 0x1400),
        "BTFSC" => (Fb, 0x1800),
        "BTFSS" => (Fb, 0x1C00),
        "ADDLW" => (K8, 0x3E00),
        "ANDLW" => (K8, 0x3900),
        "CALL" => (K11, 0x2000),
        "CLRWDT" => (Plain, 0x0064),
        "GOTO" => (K11, 0x2800),
        "IORLW" => (K8, 0x3800),
        "MOVLW" => (K8, 0x3000),
        "RETFIE" => (Plain, 0x0009),
        "RETLW" => (K8, 0x3400),
        "RETURN" => (Plain, 0x0008),
        "SLEEP" => (Plain, 0x0063),
        "SUBLW" => (K8, 0x3C00),
        "XORLW" => (K8, 0x3A00),
        "OPTION" => (Plain, 0x0062),
        "TRIS" => (F, 0x0060),
        _ => return None,
    })
}

/// Pseudobefehle: Übersetzung in echte Befehle als (Mnemonic, Operanden).
fn pseudo(name: &str, args: &[String]) -> Option<Vec<(String, String)>> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let fd = || if args.len() > 1 { format!("{},{}", a(0), a(1)) } else { a(0) };
    let one = |m: &str, o: &str| vec![(m.to_string(), o.to_string())];
    let two = |m1: &str, o1: &str, m2: &str, o2: String| {
        vec![(m1.to_string(), o1.to_string()), (m2.to_string(), o2)]
    };
    Some(match name {
        "CLRC" => one("BCF", "3,0"),
        "SETC" | "STC" => one("BSF", "3,0"),
        "CLRDC" => one("BCF", "3,1"),
        "SETDC" | "STDC" => one("BSF", "3,1"),
        "CLRZ" | "CLZ" => one("BCF", "3,2"),
        "SETZ" | "STZ" => one("BSF", "3,2"),
        "SKPC" => one("BTFSS", "3,0"),
        "SKPNC" => one("BTFSC", "3,0"),
        "SKPDC" => one("BTFSS", "3,1"),
        "SKPNDC" => one("BTFSC", "3,1"),
        "SKPZ" => one("BTFSS", "3,2"),
        "SKPNZ" => one("BTFSC", "3,2"),
        "B" | "JMP" => one("GOTO", &a(0)),
        "BC" => two("BTFSC", "3,0", "GOTO", a(0)),
        "BNC" => two("BTFSS", "3,0", "GOTO", a(0)),
        "BDC" => two("BTFSC", "3,1", "GOTO", a(0)),
        "BNDC" => two("BTFSS", "3,1", "GOTO", a(0)),
        "BZ" => two("BTFSC", "3,2", "GOTO", a(0)),
        "BNZ" => two("BTFSS", "3,2", "GOTO", a(0)),
        "MOVFW" => one("MOVF", &format!("{},0", a(0))),
        "TSTF" => one("MOVF", &format!("{},1", a(0))),
        "NEGF" => two("COMF", &format!("{},1", a(0)), "INCF", fd()),
        "ADDCF" => two("BTFSC", "3,0", "INCF", fd()),
        "SUBCF" => two("BTFSC", "3,0", "DECF", fd()),
        "ADDDCF" => two("BTFSC", "3,1", "INCF", fd()),
        "SUBDCF" => two("BTFSC", "3,1", "DECF", fd()),
        "SETB" => one("BSF", &args.join(",")),
        "CLRB" => one("BCF", &args.join(",")),
        "SNB" => one("BTFSC", &args.join(",")),
        "SB" => one("BTFSS", &args.join(",")),
        _ => return None,
    })
}

pub const DIRECTIVES: &[&str] = &[
    "EQU", "SET", "ORG", "END", "DEVICE", "CONFIG", "INCLUDE", "#INCLUDE", "LIST", "NOLIST", "DEFINE",
    "#DEFINE", "DW", "DATA", "DT", "RETW", "IF", "IFDEF", "IFNDEF", "ELSE", "ELSEIF", "ENDIF", "LGOTO",
    "LCALL", "PAGE", "TITLE", "PROCESSOR", "RADIX", "__CONFIG", "ERRORLEVEL",
];

fn is_keyword(name: &str) -> bool {
    opcode(name).is_some() || pseudo(name, &[]).is_some() || DIRECTIVES.contains(&name) || name.starts_with('$')
}

/// Ist das Wort ein Befehl oder Pseudobefehl? Für die Syntax-Hervorhebung.
pub fn is_mnemonic(word: &str) -> bool {
    let up = word.to_ascii_uppercase();
    opcode(&up).is_some() || pseudo(&up, &[]).is_some()
}

pub fn is_directive(word: &str) -> bool {
    DIRECTIVES.contains(&word.to_ascii_uppercase().as_str())
}

struct Statement {
    label: Option<String>,
    op: Option<String>,
    operands: String,
}

fn take_word(s: &str) -> (String, String) {
    let s = s.trim_start();
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    (s[..end].to_string(), s[end..].trim().to_string())
}

fn parse_statement(line: &str) -> Statement {
    let code = strip_comment(line);
    let starts_col1 = code.chars().next().is_some_and(|c| !c.is_whitespace());
    let (first, after) = take_word(code);
    if first.is_empty() {
        return Statement { label: None, op: None, operands: String::new() };
    }
    let first_up = first.trim_end_matches(':').to_ascii_uppercase();
    let (second, _) = take_word(&after);
    let next_defines = matches!(second.to_ascii_uppercase().as_str(), "EQU" | "SET" | "=");

    let is_label = first.ends_with(':') || next_defines || (starts_col1 && !is_keyword(&first_up));
    let (label, rest) = if is_label { (Some(first_up), after) } else { (None, code.trim().to_string()) };
    let (op, operands) = take_word(&rest);
    Statement {
        label,
        op: if op.is_empty() { None } else { Some(op.to_ascii_uppercase()) },
        operands,
    }
}

/// Formatiert die Quelltextzeile für das Listing wie iL_ASS16:
/// 2 Leerzeichen, Labelfeld mit 9 Zeichen, Rest mit Tabs als Leerzeichen.
fn format_source(line: &str) -> String {
    let starts_col1 = line.chars().next().is_some_and(|c| !c.is_whitespace() && c != ';');
    let (label, rest) = if starts_col1 {
        let end = line.find(char::is_whitespace).unwrap_or(line.len());
        (&line[..end], &line[end..])
    } else {
        ("", line)
    };
    let rest = rest.trim_start().replace('\t', " ");
    format!("  {label:<9}{rest}")
}

struct Asm {
    syms: Symbols,
    user_defined: HashSet<String>,
    labels: Vec<(u16, String)>,
    words: HashMap<u16, u16>,
    config: u16,
    device: String,
    pass: u8,
    errors: Vec<AsmError>,
}

impl Asm {
    fn new() -> Self {
        let mut a = Asm {
            syms: Symbols::new(),
            user_defined: HashSet::new(),
            labels: Vec::new(),
            words: HashMap::new(),
            config: 0x3FFF,
            device: "16F84".into(),
            pass: 1,
            errors: Vec::new(),
        };
        a.load_device("16F84");
        a
    }

    fn load_device(&mut self, name: &str) {
        for (k, v) in device::symbols(name) {
            self.syms.insert(k.to_string(), v);
        }
    }

    fn eval(&self, s: &str, here: u16) -> Result<i64, String> {
        match eval(s, &self.syms, here as i64) {
            Ok(v) => Ok(v),
            // Vorwärtsreferenz: erst im 2. Durchlauf bekannt
            Err(ExprError::Unknown(_)) if self.pass == 1 => Ok(0),
            Err(ExprError::Unknown(n)) => Err(format!("Symbol/Label nicht definiert o. Arg. fehlt: {n}")),
            Err(ExprError::Syntax(s)) => Err(format!("Syntax-Fehler {s}").trim_end().to_string()),
            Err(ExprError::DivZero) => Err("Division durch Null".into()),
        }
    }

    /// Operanden mit Textersatz (z.B. `GIE` -> `0BH,7`).
    fn args(&self, operands: &str) -> Vec<String> {
        let mut out = Vec::new();
        for a in split_args(operands) {
            match self.syms.get(&a.to_ascii_uppercase()) {
                Some(Value::Text(t)) if t.contains(',') => out.extend(split_args(t)),
                _ => out.push(a),
            }
        }
        out
    }

    fn define(&mut self, name: &str, value: Value, redefinable: bool) -> Result<(), String> {
        if self.pass == 1
            && !redefinable
            && self.user_defined.contains(name)
            && self.syms.get(name) != Some(&value)
        {
            return Err(format!("Symbol / Label bereits definiert: {name}"));
        }
        self.user_defined.insert(name.to_string());
        self.syms.insert(name.to_string(), value);
        Ok(())
    }

    fn equ_value(&self, operands: &str, here: u16) -> Result<Value, String> {
        if split_args(operands).len() > 1 {
            return Ok(Value::Text(operands.trim().to_string()));
        }
        let up = operands.trim().to_ascii_uppercase();
        if let Some(Value::Text(t)) = self.syms.get(&up) {
            return Ok(Value::Text(t.clone()));
        }
        self.eval(operands, here).map(Value::Num)
    }

    fn encode(&self, mnem: &str, operands: &str, here: u16) -> Result<Vec<u16>, String> {
        let args = self.args(operands);
        if let Some((kind, base)) = opcode(mnem) {
            let bad = || "Symbol/Label nicht definiert o. Arg. fehlt".to_string();
            let need = |n: usize, m: usize| {
                if args.len() < n || args.len() > m || args.iter().any(String::is_empty) {
                    Err(bad())
                } else {
                    Ok(())
                }
            };
            let f = |s: &str| -> Result<u16, String> {
                let v = self.eval(s, here)?;
                if !(0..=0x1FF).contains(&v) {
                    return Err(bad());
                }
                Ok(v as u16 & 0x7F)
            };
            let word = match kind {
                Kind::Plain => {
                    need(0, 0)?;
                    base
                }
                Kind::F => {
                    need(1, 1)?;
                    base | f(&args[0])?
                }
                Kind::Fd => {
                    need(1, 2)?;
                    let d = match args.get(1) {
                        Some(d) => self.eval(d, here)?,
                        None => 1,
                    };
                    if !(0..=1).contains(&d) {
                        return Err(bad());
                    }
                    base | (d as u16) << 7 | f(&args[0])?
                }
                Kind::Fb => {
                    need(2, 2)?;
                    let b = self.eval(&args[1], here)?;
                    if !(0..=7).contains(&b) {
                        return Err("Nur Bitadressierung [0..7] erlaubt".into());
                    }
                    base | (b as u16) << 7 | f(&args[0])?
                }
                Kind::K8 => {
                    need(1, 1)?;
                    let k = self.eval(&args[0], here)?;
                    if !(-128..=255).contains(&k) {
                        return Err("Überlauf im Ergebnis".into());
                    }
                    base | (k as u16 & 0xFF)
                }
                Kind::K11 => {
                    need(1, 1)?;
                    let k = self.eval(&args[0], here)?;
                    if !(0..0x2000).contains(&k) {
                        return Err("Adressbereich überschritten".into());
                    }
                    base | (k as u16 & 0x7FF)
                }
            };
            return Ok(vec![word]);
        }
        if let Some(parts) = pseudo(mnem, &args) {
            let mut out = Vec::new();
            for (i, (m, o)) in parts.iter().enumerate() {
                out.extend(self.encode(m, o, here + i as u16)?);
            }
            return Ok(out);
        }
        match mnem {
            "LGOTO" | "LCALL" => {
                let k = self.eval(args.first().map(String::as_str).unwrap_or(""), here)? as u16;
                let b3 = if k & 0x0800 != 0 { 0x1400 } else { 0x1000 };
                let b4 = if k & 0x1000 != 0 { 0x1400 } else { 0x1000 };
                let jump = if mnem == "LGOTO" { 0x2800 } else { 0x2000 };
                Ok(vec![b3 | 3 << 7 | 0x0A, b4 | 4 << 7 | 0x0A, jump | (k & 0x7FF)])
            }
            _ => Err(format!("Syntax-Fehler: unbekannter Befehl {mnem}")),
        }
    }

    /// Wortanzahl, ohne Symbolwerte zu kennen.
    fn size_of(&self, mnem: &str, operands: &str) -> u16 {
        match mnem {
            "LGOTO" | "LCALL" => 3,
            _ if opcode(mnem).is_some() => 1,
            _ => pseudo(mnem, &self.args(operands)).map(|p| p.len() as u16).unwrap_or(0),
        }
    }

    fn config_word(&mut self, operands: &str, here: u16) {
        for arg in split_args(operands) {
            let (mask, val): (u16, u16) = match arg.to_ascii_uppercase().as_str() {
                "WDT_ON" => (0x0004, 0x0004),
                "WDT_OFF" => (0x0004, 0),
                "PWRT_ON" => (0x0008, 0),
                "PWRT_OFF" => (0x0008, 0x0008),
                "CP_ON" => (0x3FF0, 0),
                "CP_OFF" => (0x3FF0, 0x3FF0),
                "LP_OSC" => (0x0003, 0),
                "XT_OSC" => (0x0003, 1),
                "HS_OSC" => (0x0003, 2),
                "RC_OSC" => (0x0003, 3),
                _ => match eval(&arg, &self.syms, here as i64) {
                    Ok(v) => (0x3FFF, v as u16 & 0x3FFF),
                    // Optionen anderer PIC-Typen (z.B. ANSEL) werden ignoriert
                    Err(_) => (0, 0),
                },
            };
            self.config = (self.config & !mask) | val;
        }
    }

    fn put(&mut self, addr: u16, word: u16) -> Result<(), String> {
        if addr >= PROG_WORDS {
            return Err("Programm zu groß".into());
        }
        if self.pass == 2 && self.words.insert(addr, word & 0x3FFF).is_some() {
            return Err("Programmspeicher bereits mit Code belegt".into());
        }
        Ok(())
    }

    /// Ein Durchlauf über alle Zeilen. Liefert pro Zeile die erzeugten Wörter.
    fn run(&mut self, lines: &[SrcLine]) -> Vec<Vec<(u16, u16)>> {
        let mut pc: u16 = 0;
        let mut cond: Vec<(bool, bool)> = Vec::new(); // (aktiv, Zweig schon genommen)
        let mut ended = false;
        let mut out = Vec::with_capacity(lines.len());

        for (idx, line) in lines.iter().enumerate() {
            let mut words = Vec::new();
            if !ended {
                if let Err(msg) = self.line(&line.text, &mut pc, &mut cond, &mut ended, &mut words) {
                    // Unbekannte Symbole meldet erst der 2. Durchlauf
                    if self.pass == 2 || !msg.starts_with("Symbol/Label") {
                        self.errors.push(AsmError {
                            line: idx + 1,
                            file: line.file.clone(),
                            src_line: line.lineno,
                            warning: msg.starts_with("Warnung"),
                            msg,
                        });
                    }
                }
            }
            out.push(words);
        }
        if !cond.is_empty() {
            let last = lines.last();
            self.errors.push(AsmError {
                line: lines.len(),
                file: last.map(|l| l.file.clone()).unwrap_or_default(),
                src_line: last.map(|l| l.lineno).unwrap_or(0),
                msg: "Kein ENDIF vorhanden".into(),
                warning: false,
            });
        }
        out
    }

    fn line(
        &mut self,
        text: &str,
        pc: &mut u16,
        cond: &mut Vec<(bool, bool)>,
        ended: &mut bool,
        words: &mut Vec<(u16, u16)>,
    ) -> Result<(), String> {
        let st = parse_statement(text);
        let active = cond.iter().all(|c| c.0);
        let op = st.op.clone().unwrap_or_default();

        match op.as_str() {
            "IF" | "IFDEF" | "IFNDEF" => {
                let v = if !active {
                    false
                } else if op == "IF" {
                    self.eval(&st.operands, *pc)? != 0
                } else {
                    let def = self.syms.contains_key(&st.operands.trim().to_ascii_uppercase());
                    def == (op == "IFDEF")
                };
                // Bei inaktivem Elternzweig gilt der ganze Block als erledigt
                cond.push((v, v || !active));
                return Ok(());
            }
            "ELSEIF" => {
                let Some(top) = cond.last().copied() else {
                    return Err("ELSEIF ohne IF".into());
                };
                let v = !top.1 && self.eval(&st.operands, *pc)? != 0;
                *cond.last_mut().unwrap() = (v, top.1 || v);
                return Ok(());
            }
            "ELSE" => {
                let Some(top) = cond.last().copied() else {
                    return Err("ELSE ohne IF".into());
                };
                *cond.last_mut().unwrap() = (!top.1, true);
                return Ok(());
            }
            "ENDIF" => {
                return cond.pop().map(|_| ()).ok_or_else(|| "ENDIF ohne IF".into());
            }
            _ => {}
        }
        if !active {
            return Ok(());
        }

        match op.as_str() {
            "EQU" | "=" | "SET" => {
                let name = st.label.clone().ok_or("Syntax-Fehler: Name fehlt")?;
                let v = self.equ_value(&st.operands, *pc)?;
                return self.define(&name, v, op == "SET");
            }
            "DEFINE" | "#DEFINE" => {
                let (name, val) = take_word(&st.operands);
                let val = if val.is_empty() { "1".to_string() } else { val };
                let v = self.equ_value(&val, *pc).unwrap_or(Value::Text(val));
                return self.define(&name.to_ascii_uppercase(), v, true);
            }
            _ => {}
        }

        if let Some(label) = st.label.clone() {
            self.define(&label, Value::Num(*pc as i64), false)?;
            if self.pass == 2 {
                self.labels.push((*pc, label));
            }
        }

        match op.as_str() {
            "" | "INCLUDE" | "#INCLUDE" | "LIST" | "NOLIST" | "PAGE" | "TITLE" | "RADIX" | "ERRORLEVEL" => Ok(()),
            "ORG" => {
                let v = self.eval(&st.operands, *pc)?;
                if !(0..PROG_WORDS as i64).contains(&v) {
                    return Err("gültiger Adressbereich überschritten".into());
                }
                *pc = v as u16;
                Ok(())
            }
            "END" => {
                *ended = true;
                Ok(())
            }
            "DEVICE" | "PROCESSOR" => {
                let d = st.operands.trim().to_ascii_uppercase();
                let d = d.trim_start_matches("PIC").to_string();
                self.device = d.clone();
                if device::supported(&d) {
                    Ok(())
                } else {
                    Err(format!("Warnung: {d} wird nicht unterstützt, simuliert wird ein 16F84"))
                }
            }
            "CONFIG" | "__CONFIG" => {
                self.config_word(&st.operands, *pc);
                Ok(())
            }
            "DW" | "DATA" | "DT" | "RETW" => {
                let retlw = op == "DT" || op == "RETW";
                for a in self.args(&st.operands) {
                    // Text in Anführungszeichen: ein Wort je Zeichen
                    let values: Vec<u16> = match a.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                        Some(text) => text.chars().map(|c| c as u16).collect(),
                        None => vec![self.eval(&a, *pc)? as u16],
                    };
                    for v in values {
                        let w = if retlw { 0x3400 | (v & 0xFF) } else { v & 0x3FFF };
                        self.put(*pc, w)?;
                        words.push((*pc, w));
                        *pc += 1;
                    }
                }
                Ok(())
            }
            m if m.starts_with('$') => Ok(()), // $DEBUG u.ä. aus iL_ASS16
            m => {
                let enc = if self.pass == 1 {
                    match self.size_of(m, &st.operands) {
                        0 => return Err(format!("Syntax-Fehler: unbekannter Befehl {m}")),
                        n => vec![0; n as usize],
                    }
                } else {
                    self.encode(m, &st.operands, *pc)?
                };
                for w in enc {
                    self.put(*pc, w)?;
                    words.push((*pc, w));
                    *pc += 1;
                }
                Ok(())
            }
        }
    }
}

/// Übersetzt Quelltext. `dir` ist das Verzeichnis für INCLUDE-Dateien.
pub fn assemble(src: &str, file_name: &str, dir: Option<&Path>) -> Output {
    let mut lines = Vec::new();
    let mut inc_errors = Vec::new();
    expand_includes(src, file_name, dir, 0, &mut lines, &mut inc_errors);

    let mut asm = Asm::new();
    // Gerät vorab bestimmen, damit die passenden Symbole geladen sind
    if let Some(d) = lines.iter().find_map(|l| {
        let st = parse_statement(&l.text);
        matches!(st.op.as_deref(), Some("DEVICE") | Some("PROCESSOR")).then_some(st.operands)
    }) {
        asm.load_device(d.trim().to_ascii_uppercase().trim_start_matches("PIC"));
    }

    asm.pass = 1;
    asm.run(&lines);
    let mut errors: Vec<_> = asm.errors.drain(..).collect();
    asm.pass = 2;
    let per_line = asm.run(&lines);

    for e in asm.errors.drain(..) {
        if !errors.iter().any(|x| x.line == e.line && x.msg == e.msg) {
            errors.push(e);
        }
    }
    for (line, msg) in inc_errors {
        let l = &lines[line.saturating_sub(1)];
        errors.push(AsmError { line, file: l.file.clone(), src_line: l.lineno, msg, warning: false });
    }
    errors.sort_by_key(|e| e.line);

    let mut words: Vec<(u16, u16)> = asm.words.iter().map(|(&a, &w)| (a, w)).collect();
    words.sort();

    Output {
        lst: build_lst(&lines, &per_line, &errors),
        hex: build_hex(&words, asm.config),
        err: build_err(&errors, words.len()),
        sym: build_sym(&asm.labels),
        words,
        errors,
        config: asm.config,
        device: asm.device,
    }
}

fn build_lst(lines: &[SrcLine], per_line: &[Vec<(u16, u16)>], errors: &[AsmError]) -> String {
    let mut s = String::new();
    for (i, (line, words)) in lines.iter().zip(per_line).enumerate() {
        let n = i + 1;
        let src = format_source(&line.text);
        match words.first() {
            Some((a, w)) => s.push_str(&format!("{a:04X} {w:04X}           {n:05}{src}\r\n")),
            None => s.push_str(&format!("{:20}{n:05}{src}\r\n", "")),
        }
        for (a, w) in words.iter().skip(1) {
            s.push_str(&format!("{a:04X} {w:04X}\r\n"));
        }
        for e in errors.iter().filter(|e| e.line == n) {
            let kind = if e.warning { "Warnung" } else { "Fehler" };
            s.push_str(&format!("{kind} in Zeile {} : {}!\r\n", e.src_line, e.msg));
        }
    }
    s
}

fn hex_record(addr: u16, kind: u8, data: &[u8]) -> String {
    let mut sum = (data.len() as u8)
        .wrapping_add((addr >> 8) as u8)
        .wrapping_add(addr as u8)
        .wrapping_add(kind);
    let mut s = format!(":{:02X}{addr:04X}{kind:02X}", data.len());
    for b in data {
        sum = sum.wrapping_add(*b);
        s.push_str(&format!("{b:02X}"));
    }
    s.push_str(&format!("{:02X}\r\n", sum.wrapping_neg()));
    s
}

/// Intel-HEX (INHX8M): Wörter little endian, Byteadresse = Wortadresse * 2.
pub fn build_hex(words: &[(u16, u16)], config: u16) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < words.len() {
        let start = words[i].0;
        let mut data = Vec::new();
        let mut j = i;
        while j < words.len() && words[j].0 == start + (j - i) as u16 && j - i < 8 {
            data.extend_from_slice(&words[j].1.to_le_bytes());
            j += 1;
        }
        s.push_str(&hex_record(start * 2, 0, &data));
        i = j;
    }
    s.push_str(&hex_record(CONFIG_ADDR * 2, 0, &config.to_le_bytes()));
    s.push_str(":00000001FF\r\n");
    s
}

fn build_sym(labels: &[(u16, String)]) -> String {
    let mut l = labels.to_vec();
    l.sort();
    let mut s = String::from("*LABELS*\r\n");
    for (a, n) in l {
        s.push_str(&format!("{a:04X} {n}\r\n"));
    }
    s
}

fn build_err(errors: &[AsmError], used: usize) -> String {
    let mut s = String::new();
    for e in errors {
        let kind = if e.warning { "Warnung" } else { "Fehler" };
        s.push_str(&format!("{kind} in Zeile {} ({}): {}\r\n", e.src_line, e.file, e.msg));
    }
    s.push_str(&format!(
        "Using {used} ({used:04X}H) Words of {PROG_WORDS} ({PROG_WORDS:04X}H) Words\r\n"
    ));
    s
}

/// Übersetzt Quelltext und schreibt LST, HEX, ERR und SYM neben `path`.
/// Liefert das Ergebnis und den Pfad der LST-Datei.
pub fn assemble_source_to(src: &str, path: &Path) -> std::io::Result<(Output, PathBuf)> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let out = assemble(src, &name, path.parent());
    let lst = path.with_extension("LST");
    std::fs::write(&lst, latin1(&out.lst))?;
    std::fs::write(path.with_extension("HEX"), &out.hex)?;
    std::fs::write(path.with_extension("ERR"), latin1(&out.err))?;
    std::fs::write(path.with_extension("SYM"), &out.sym)?;
    Ok((out, lst))
}

/// Wie `assemble_source_to`, liest den Quelltext aus `path`.
pub fn assemble_file(path: &Path) -> std::io::Result<(Output, PathBuf)> {
    let src = decode(&std::fs::read(path)?);
    assemble_source_to(&src, path)
}

/// Schreibt wie das Original in Latin-1, damit alte Werkzeuge Umlaute lesen.
fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asm(src: &str) -> Output {
        assemble(src, "test.src", None)
    }

    fn code(src: &str) -> Vec<u16> {
        let o = asm(src);
        assert!(o.ok(), "{:?}", o.errors);
        o.words.iter().map(|w| w.1).collect()
    }

    #[test]
    fn statement_parsing() {
        let s = parse_statement("loop\tbtfsc portb,key ; Kommentar");
        assert_eq!(s.label.as_deref(), Some("LOOP"));
        assert_eq!(s.op.as_deref(), Some("BTFSC"));
        assert_eq!(s.operands, "portb,key");
        let s = parse_statement("\tnop");
        assert_eq!(s.label, None);
        let s = parse_statement("nop");
        assert_eq!(s.label, None);
        assert_eq!(s.op.as_deref(), Some("NOP"));
        let s = parse_statement("status\tequ\t3");
        assert_eq!(s.label.as_deref(), Some("STATUS"));
        assert_eq!(s.op.as_deref(), Some("EQU"));
    }

    #[test]
    fn basic_instructions() {
        assert_eq!(
            code("\tmovlw 0FFH\n\tmovwf 20h\n\tdecfsz 20h,f\n\tgoto $-1\n\tretlw 'A'\n"),
            vec![0x30FF, 0x00A0, 0x0BA0, 0x2802, 0x3441]
        );
    }

    #[test]
    fn default_destination_is_f() {
        assert_eq!(code("\tincf 20h\n\tincf 20h,w\n"), vec![0x0AA0, 0x0A20]);
    }

    #[test]
    fn predefined_bit_symbols() {
        // GIE ist als "0BH,7" definiert
        assert_eq!(code("\tbsf GIE\n\tbcf STATUS,RP0\n\tbtfss INTCON,2\n"), vec![0x178B, 0x1283, 0x1D0B]);
    }

    #[test]
    fn user_bit_symbols() {
        assert_eq!(code("TASTE1 EQU 7,4\n\tbtfsc TASTE1\n"), vec![0x1A07]);
    }

    #[test]
    fn bank1_registers_are_masked() {
        assert_eq!(code("\tmovwf TRISB\n\tclrf OPTION_R\n"), vec![0x0086, 0x0181]);
    }

    #[test]
    fn forward_labels_and_org() {
        let o = asm("\torg 0\n\tgoto start\n\torg 4\nstart\tnop\n\tgoto start\n");
        assert!(o.ok());
        assert_eq!(o.words, vec![(0, 0x2804), (4, 0x0000), (5, 0x2804)]);
    }

    #[test]
    fn pseudo_instructions() {
        assert_eq!(code("\tskpz\n\tclrc\n\tmovfw 20h\n\tbz 0\n"), vec![0x1D03, 0x1003, 0x0820, 0x1903, 0x2800]);
    }

    #[test]
    fn conditional_assembly() {
        let src = "X EQU 1\n\tIF X = 1\n\tnop\n\tELSE\n\tsleep\n\tENDIF\n\
                   \tIF X = 2\n\tsleep\n\tELSEIF X = 1\n\tclrw\n\tENDIF\n\
                   \tIF 0\n\tIF 1\n\tsleep\n\tENDIF\n\tENDIF\n";
        assert_eq!(code(src), vec![0x0000, 0x0100]);
    }

    #[test]
    fn dt_with_text() {
        assert_eq!(code("\tdt \"Hi\",0\n\tretw 1,2\n"), vec![0x3448, 0x3469, 0x3400, 0x3401, 0x3402]);
    }

    #[test]
    fn examples_assemble() {
        for src in [include_str!("../../../examples/lcd_hallo.src"), include_str!("../../../examples/ampel.src")] {
            let o = asm(src);
            assert!(o.ok(), "{:?}", o.errors);
        }
    }

    #[test]
    fn errors_are_reported() {
        let o = asm("\tgoto nirgends\n\tbsf 3,9\n\tfoo 1\n\tnop\n");
        let msgs: Vec<_> = o.errors.iter().map(|e| (e.src_line, e.msg.clone())).collect();
        assert_eq!(msgs.len(), 3, "{msgs:?}");
        assert_eq!(msgs[0].0, 1);
        assert!(msgs[0].1.contains("NIRGENDS"));
        assert!(msgs[1].1.contains("Bitadressierung"));
        assert!(msgs[2].1.contains("unbekannter Befehl"));
    }

    #[test]
    fn duplicate_label() {
        let o = asm("a\tnop\na\tnop\n");
        assert!(o.errors.iter().any(|e| e.msg.contains("bereits definiert")));
    }

    #[test]
    fn config_word() {
        let o = asm("\tconfig wdt_off, xt_osc\n\tnop\n");
        assert_eq!(o.config, 0x3FF9);
        assert!(o.hex.contains(":02400E00F93F"));
    }

    #[test]
    fn lst_format_matches_simulator() {
        let o = asm("\tdevice 16f84\nloop\tgoto loop\n");
        let lines: Vec<_> = o.lst.lines().collect();
        assert_eq!(lines[0], "                    00001           device 16f84");
        assert_eq!(lines[1], "0000 2800           00002  loop     goto loop");
    }

    /// Vergleich mit LST-Dateien, die das Original iL_ASS16 erzeugt hat.
    fn compare_with_original(src: &[u8], lst: &[u8]) -> Output {
        let original: Vec<(u16, u16)> = decode(lst)
            .lines()
            .filter_map(|l| {
                let a = u16::from_str_radix(l.get(0..4)?, 16).ok()?;
                let w = u16::from_str_radix(l.get(5..9)?, 16).ok()?;
                Some((a, w))
            })
            .collect();
        let o = asm(&decode(src));
        assert!(o.ok(), "{:?}", o.errors);
        assert_eq!(o.words, original);
        let orig = decode(lst);
        let orig_lst: Vec<_> = orig.lines().collect();
        let new_lst: Vec<_> = o.lst.lines().collect();
        assert_eq!(new_lst.len(), orig_lst.len());
        for (n, o) in new_lst.iter().zip(&orig_lst) {
            assert_eq!(n.get(0..25), o.get(0..25), "Adresse/Opcode/Zeilennummer");
        }
        o
    }

    #[test]
    fn same_code_as_original_testo01() {
        let o = compare_with_original(
            include_bytes!("../../../examples/Testo01.src"),
            include_bytes!("../../../examples/Testo01.LST"),
        );
        assert_eq!(o.sym, "*LABELS*\r\n0000 COLD\r\n0000 INIT\r\n0003 LOOP\r\n0005 LEDEIN\r\n0007 LEDAUS\r\n");
    }

    #[test]
    fn same_code_as_original_all_instructions() {
        compare_with_original(
            include_bytes!("../../../examples/alle_befehle.src"),
            include_bytes!("../../../examples/alle_befehle.LST"),
        );
    }

    #[test]
    fn hex_checksums() {
        let h = build_hex(&[(0, 0x1683), (1, 0x1005)], 0x3FFF);
        assert!(h.starts_with(":0400000083160510"));
        assert!(h.ends_with(":00000001FF\r\n"));
    }
}
