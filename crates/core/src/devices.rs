//! Peripherie, die jeden Befehlszyklus beobachten muss: LCD, Siebensegment,
//! Funktionsgenerator, Schrittmotor. LEDs, Schalter und Taster sind so
//! einfach, dass sie direkt im Board stecken.

use crate::ini::Ini;
use crate::pins::PinRef;

/// Pegel eines Pins aus Sicht eines Geräts.
pub fn level(pins: &[u8; 5], p: PinRef) -> Option<bool> {
    match p {
        PinRef::Nc => None,
        PinRef::Vdd => Some(true),
        PinRef::Gnd => Some(false),
        PinRef::Port { port, bit } => Some(pins[port as usize] >> bit & 1 != 0),
    }
}

// ----------------------------------------------------------------------
// LCD (HD44780)

/// Anzeigeformate in der Reihenfolge von FORMAT_1..FORMAT_8 der INI.
pub const LCD_FORMATS: [(usize, usize); 8] =
    [(1, 8), (1, 16), (1, 20), (2, 16), (2, 20), (2, 40), (4, 16), (4, 20)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LcdColor {
    #[default]
    Grau,
    Gelb,
    Blau,
}

#[derive(Debug, Clone, Default)]
pub struct LcdConfig {
    pub e: PinRef,
    pub rs: PinRef,
    pub rw: PinRef,
    /// D0..D7, im 4-Bit-Modus sind D0..D3 nc.
    pub d: [PinRef; 8],
    pub format: usize,
    pub color: LcdColor,
}

impl LcdConfig {
    pub fn from_ini(ini: &Ini) -> Self {
        let mut d = [PinRef::Nc; 8];
        for (i, p) in d.iter_mut().enumerate() {
            *p = ini.pin("LCD", &format!("D{i}"));
        }
        let format = (1..=8).find(|i| ini.flag("LCD", &format!("FORMAT_{i}"))).unwrap_or(4) - 1;
        let color = if ini.flag("LCD", "gelb") {
            LcdColor::Gelb
        } else if ini.flag("LCD", "blau") {
            LcdColor::Blau
        } else {
            LcdColor::Grau
        };
        LcdConfig { e: ini.pin("LCD", "E"), rs: ini.pin("LCD", "RS"), rw: ini.pin("LCD", "RW"), d, format, color }
    }

    pub fn to_ini(&self, ini: &mut Ini) {
        ini.set_pin("LCD", "E", self.e);
        ini.set_pin("LCD", "RS", self.rs);
        ini.set_pin("LCD", "RW", self.rw);
        for (i, p) in self.d.iter().enumerate() {
            ini.set_pin("LCD", &format!("D{i}"), *p);
        }
        ini.set_flag("LCD", "grau", self.color == LcdColor::Grau);
        ini.set_flag("LCD", "gelb", self.color == LcdColor::Gelb);
        ini.set_flag("LCD", "blau", self.color == LcdColor::Blau);
        for i in 0..8 {
            ini.set_flag("LCD", &format!("FORMAT_{}", i + 1), i == self.format);
        }
    }

    pub fn size(&self) -> (usize, usize) {
        LCD_FORMATS[self.format.min(7)]
    }
}

#[derive(Debug, Clone)]
pub struct Lcd {
    pub cfg: LcdConfig,
    pub ddram: [u8; 128],
    pub cgram: [u8; 64],
    pub display_on: bool,
    pub cursor_on: bool,
    pub blink_on: bool,
    pub addr: u8,
    pub cg_mode: bool,
    pub increment: bool,
    pub shift_display: bool,
    pub shift: i32,
    pub four_bit: bool,
    pub two_lines: bool,
    high_nibble: Option<u8>,
    last_e: bool,
}

impl Lcd {
    pub fn new(cfg: LcdConfig) -> Self {
        Lcd {
            cfg,
            ddram: [b' '; 128],
            cgram: [0; 64],
            display_on: false,
            cursor_on: false,
            blink_on: false,
            addr: 0,
            cg_mode: false,
            increment: true,
            shift_display: false,
            shift: 0,
            four_bit: false,
            two_lines: false,
            high_nibble: None,
            last_e: false,
        }
    }

    /// Aus- und wieder einschalten (bei Reset der Simulation).
    pub fn power_on(&mut self) {
        let cfg = self.cfg.clone();
        *self = Lcd::new(cfg);
    }

    /// Beobachtet die Pins. Übernahme mit fallender Flanke an E.
    pub fn observe(&mut self, pins: &[u8; 5]) {
        let Some(e) = level(pins, self.cfg.e) else { return };
        let falling = self.last_e && !e;
        self.last_e = e;
        if !falling {
            return;
        }
        if level(pins, self.cfg.rw) == Some(true) {
            return; // Lesen: Busy-Flag ist in der Simulation immer 0
        }
        let rs = level(pins, self.cfg.rs).unwrap_or(false);
        let mut bus = 0u8;
        for (i, p) in self.cfg.d.iter().enumerate() {
            if level(pins, *p) == Some(true) {
                bus |= 1 << i;
            }
        }
        if self.four_bit {
            let nib = bus >> 4;
            match self.high_nibble.take() {
                None => self.high_nibble = Some(nib),
                Some(h) => self.write(rs, h << 4 | nib),
            }
        } else {
            self.write(rs, bus);
        }
    }

    fn write(&mut self, rs: bool, v: u8) {
        if rs {
            if self.cg_mode {
                self.cgram[(self.addr & 0x3F) as usize] = v & 0x1F;
                let next = if self.increment { self.addr.wrapping_add(1) } else { self.addr.wrapping_sub(1) };
                self.addr = next & 0x3F;
            } else {
                self.ddram[(self.addr & 0x7F) as usize] = v;
                self.move_by(if self.increment { 1 } else { -1 });
                if self.shift_display {
                    self.shift += if self.increment { 1 } else { -1 };
                }
            }
            return;
        }
        if v & 0x80 != 0 {
            self.cg_mode = false;
            self.addr = v & 0x7F;
        } else if v & 0x40 != 0 {
            self.cg_mode = true;
            self.addr = v & 0x3F;
        } else if v & 0x20 != 0 {
            self.four_bit = v & 0x10 == 0;
            self.two_lines = v & 0x08 != 0;
            self.high_nibble = None;
        } else if v & 0x10 != 0 {
            let right = v & 0x04 != 0;
            if v & 0x08 != 0 {
                self.shift += if right { 1 } else { -1 };
            } else {
                self.move_by(if right { 1 } else { -1 });
            }
        } else if v & 0x08 != 0 {
            self.display_on = v & 0x04 != 0;
            self.cursor_on = v & 0x02 != 0;
            self.blink_on = v & 0x01 != 0;
        } else if v & 0x04 != 0 {
            self.increment = v & 0x02 != 0;
            self.shift_display = v & 0x01 != 0;
        } else if v & 0x02 != 0 {
            self.addr = 0;
            self.cg_mode = false;
            self.shift = 0;
        } else if v & 0x01 != 0 {
            self.ddram = [b' '; 128];
            self.addr = 0;
            self.cg_mode = false;
            self.shift = 0;
            self.increment = true;
        }
    }

    fn move_by(&mut self, d: i32) {
        if self.two_lines {
            // Zeile 1: 00..27, Zeile 2: 40..67
            let col = (self.addr & 0x3F) as i32 + d;
            let line = if !(0..40).contains(&col) { (self.addr & 0x40) ^ 0x40 } else { self.addr & 0x40 };
            self.addr = line | col.rem_euclid(40) as u8;
        } else {
            self.addr = (self.addr as i32 + d).rem_euclid(80) as u8;
        }
    }

    /// DDRAM-Adresse des Zeichens in Zeile/Spalte der Anzeige.
    pub fn ddram_addr(&self, row: usize, col: usize) -> u8 {
        let cols = self.cfg.size().1 as i32;
        let (line, start) = match row {
            0 => (0x00, 0),
            1 => (0x40, 0),
            2 => (0x00, cols),
            _ => (0x40, cols),
        };
        let len = if self.two_lines { 40 } else { 80 };
        (line + (start + col as i32 + self.shift).rem_euclid(len)) as u8
    }

    /// Zeichencode an einer Position oder None, wenn die Anzeige aus ist.
    pub fn char_at(&self, row: usize, col: usize) -> Option<u8> {
        if !self.display_on {
            return None;
        }
        if row % 2 == 1 && !self.two_lines {
            return Some(b' ');
        }
        Some(self.ddram[self.ddram_addr(row, col) as usize & 0x7F])
    }

    /// Steht der Cursor an dieser Position?
    pub fn cursor_at(&self, row: usize, col: usize) -> bool {
        self.display_on && !self.cg_mode && self.ddram_addr(row, col) == self.addr
    }

    /// 5x8-Muster eines eigenen Zeichens (0..7).
    pub fn custom_char(&self, code: u8) -> [u8; 8] {
        let base = (code as usize & 7) * 8;
        let mut out = [0; 8];
        out.copy_from_slice(&self.cgram[base..base + 8]);
        out
    }

    pub fn text(&self) -> Vec<String> {
        let (rows, cols) = self.cfg.size();
        (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| match self.char_at(r, c) {
                        Some(b) if (0x20..0x7F).contains(&b) => b as char,
                        _ => ' ',
                    })
                    .collect()
            })
            .collect()
    }
}

// ----------------------------------------------------------------------
// Siebensegmentanzeigen

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SegColor {
    #[default]
    Rot,
    Gruen,
    Blau,
    Gelb,
}

#[derive(Debug, Clone, Default)]
pub struct SevenSegConfig {
    /// Segmente a, b, c, d, e, f, g, dp.
    pub seg: [PinRef; 8],
    pub digit: [PinRef; 8],
    pub common_anode: bool,
    pub digit_inverted: bool,
    pub latch: [bool; 8],
    pub color: SegColor,
}

impl SevenSegConfig {
    pub fn from_ini(ini: &Ini) -> Self {
        let mut c = SevenSegConfig::default();
        for i in 0..8 {
            c.seg[i] = ini.pin("SSEG", &format!("PIN_{}", i + 1));
            c.digit[i] = ini.pin("SSEG", &format!("DIGIT_{}", i + 1));
            c.latch[i] = ini.flag("SSEG", &format!("LATCH_{}", i + 1));
        }
        c.common_anode = ini.flag("SSEG", "CA");
        c.digit_inverted = ini.flag("SSEG", "DIGITINV");
        c.color = if ini.flag("SSEG", "GRUEN") {
            SegColor::Gruen
        } else if ini.flag("SSEG", "BLAU") {
            SegColor::Blau
        } else if ini.flag("SSEG", "GELB") {
            SegColor::Gelb
        } else {
            SegColor::Rot
        };
        c
    }

    pub fn to_ini(&self, ini: &mut Ini) {
        for i in 0..8 {
            ini.set_pin("SSEG", &format!("PIN_{}", i + 1), self.seg[i]);
            ini.set_pin("SSEG", &format!("DIGIT_{}", i + 1), self.digit[i]);
            ini.set_flag("SSEG", &format!("LATCH_{}", i + 1), self.latch[i]);
        }
        ini.set_flag("SSEG", "ROT", self.color == SegColor::Rot);
        ini.set_flag("SSEG", "GRUEN", self.color == SegColor::Gruen);
        ini.set_flag("SSEG", "BLAU", self.color == SegColor::Blau);
        ini.set_flag("SSEG", "GELB", self.color == SegColor::Gelb);
        ini.set_flag("SSEG", "DIGITINV", self.digit_inverted);
        ini.set_flag("SSEG", "CA", self.common_anode);
    }
}

#[derive(Debug, Clone, Default)]
pub struct SevenSeg {
    pub cfg: SevenSegConfig,
    /// Leuchtdauer je Stelle und Segment seit dem letzten `take_frame`.
    on_cycles: [[u32; 8]; 8],
    total: u32,
    latched: [u8; 8],
}

impl SevenSeg {
    pub fn new(cfg: SevenSegConfig) -> Self {
        SevenSeg { cfg, ..Default::default() }
    }

    fn segments(&self, pins: &[u8; 5]) -> u8 {
        let mut s = 0;
        for (i, p) in self.cfg.seg.iter().enumerate() {
            if let Some(l) = level(pins, *p) {
                if l != self.cfg.common_anode {
                    s |= 1 << i;
                }
            }
        }
        s
    }

    fn digit_active(&self, pins: &[u8; 5], d: usize) -> bool {
        level(pins, self.cfg.digit[d]).is_some_and(|l| l != self.cfg.digit_inverted)
    }

    pub fn observe(&mut self, pins: &[u8; 5], cycles: u32) {
        let segs = self.segments(pins);
        self.total += cycles;
        for d in 0..8 {
            if !self.digit_active(pins, d) {
                continue;
            }
            if self.cfg.latch[d] {
                self.latched[d] = segs;
            }
            for s in 0..8 {
                if segs >> s & 1 != 0 {
                    self.on_cycles[d][s] += cycles;
                }
            }
        }
    }

    /// Helligkeit 0..1 je Stelle und Segment seit dem letzten Aufruf.
    /// Ohne neue Zyklen (Simulation steht) zählt der aktuelle Pinzustand.
    pub fn take_frame(&mut self, pins: &[u8; 5], out: &mut [[f32; 8]; 8]) {
        if self.total == 0 {
            self.observe(pins, 1);
        }
        let digits = self.cfg.digit.iter().filter(|p| p.is_connected()).count().max(1) as f32;
        for d in 0..8 {
            for s in 0..8 {
                out[d][s] = if self.cfg.latch[d] {
                    (self.latched[d] >> s & 1) as f32
                } else {
                    let ratio = self.on_cycles[d][s] as f32 / self.total.max(1) as f32;
                    (ratio * digits).min(1.0)
                };
            }
        }
        self.on_cycles = [[0; 8]; 8];
        self.total = 0;
    }

    pub fn reset(&mut self) {
        self.on_cycles = [[0; 8]; 8];
        self.total = 0;
        self.latched = [0; 8];
    }
}

// ----------------------------------------------------------------------
// Funktionsgenerator (nur Rechteck auf Kanal 1, wie im Original)

#[derive(Debug, Clone)]
pub struct FuncGen {
    pub pin: PinRef,
    pub freq_khz: f64,
    pub high: f64,
    pub low: f64,
}

impl Default for FuncGen {
    fn default() -> Self {
        FuncGen { pin: PinRef::Nc, freq_khz: 1.0, high: 50.0, low: 50.0 }
    }
}

fn parse_float(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

impl FuncGen {
    pub fn from_ini(ini: &Ini, section: &str) -> Self {
        let (high, low) = ini
            .get(section, "DUTY")
            .and_then(|d| d.split_once(':'))
            .and_then(|(a, b)| Some((parse_float(a)?, parse_float(b)?)))
            .unwrap_or((50.0, 50.0));
        FuncGen { pin: ini.pin(section, "PIN"), freq_khz: ini.number(section, "FREQ").unwrap_or(1.0), high, low }
    }

    pub fn to_ini(&self, ini: &mut Ini, section: &str) {
        ini.set_pin(section, "PIN", self.pin);
        ini.set(section, "FREQ", format!("{:.3}", self.freq_khz));
        ini.set(section, "DUTY", format!("{}:{}", self.high, self.low));
    }

    /// Pegel zum Zeitpunkt `t_us`.
    pub fn level_at(&self, t_us: f64) -> bool {
        if self.freq_khz <= 0.0 || self.high + self.low <= 0.0 {
            return false;
        }
        let period = 1000.0 / self.freq_khz;
        t_us.rem_euclid(period) / period < self.high / (self.high + self.low)
    }
}

// ----------------------------------------------------------------------
// Schrittmotor (unipolar, 4 Spulen)

#[derive(Debug, Clone, Default)]
pub struct Stepper {
    pub pins: [PinRef; 4],
    /// Winkel des Rotors in Grad (0 = oben), folgt dem Magnetfeld.
    pub angle: f32,
    /// Gezählte Halbschritte (positiv im Uhrzeigersinn).
    pub steps: i64,
    last_dir: Option<i32>,
}

impl Stepper {
    pub fn from_ini(ini: &Ini) -> Self {
        let mut s = Stepper::default();
        for i in 0..4 {
            s.pins[i] = ini.pin("STEPPER", &format!("PIN_{}", i + 1));
        }
        s
    }

    pub fn to_ini(&self, ini: &mut Ini) {
        for i in 0..4 {
            ini.set_pin("STEPPER", &format!("PIN_{}", i + 1), self.pins[i]);
        }
    }

    pub fn observe(&mut self, pins: &[u8; 5]) {
        let (mut x, mut y) = (0i32, 0i32);
        for (i, p) in self.pins.iter().enumerate() {
            if level(pins, *p) == Some(true) {
                match i {
                    0 => y += 1,
                    1 => x += 1,
                    2 => y -= 1,
                    _ => x -= 1,
                }
            }
        }
        if x == 0 && y == 0 {
            return;
        }
        // 8 Richtungen in 45°-Schritten
        let dir = ((x as f32).atan2(y as f32).to_degrees().rem_euclid(360.0) / 45.0).round() as i32 % 8;
        if let Some(last) = self.last_dir {
            let diff = (dir - last + 12).rem_euclid(8) - 4;
            if diff.abs() < 4 {
                self.steps += diff as i64;
            }
        }
        self.last_dir = Some(dir);
        self.angle = dir as f32 * 45.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcd_4bit() -> Lcd {
        // E=RA2, RS=RA3, D4..D7=RB4..RB7
        let mut cfg = LcdConfig { e: PinRef::port(0, 2), rs: PinRef::port(0, 3), format: 3, ..Default::default() };
        for i in 0..4 {
            cfg.d[4 + i] = PinRef::port(1, 4 + i as u8);
        }
        Lcd::new(cfg)
    }

    fn pulse(lcd: &mut Lcd, rs: bool, nibble: u8) {
        let mut pins = [0u8; 5];
        pins[1] = nibble << 4;
        pins[0] = if rs { 0x08 } else { 0 } | 0x04;
        lcd.observe(&pins);
        pins[0] &= !0x04;
        lcd.observe(&pins);
    }

    fn send(lcd: &mut Lcd, rs: bool, v: u8) {
        pulse(lcd, rs, v >> 4);
        pulse(lcd, rs, v & 0x0F);
    }

    #[test]
    fn lcd_needs_init_sequence() {
        let mut lcd = lcd_4bit();
        // Ohne Init: 8-Bit-Modus, Anzeige aus
        assert_eq!(lcd.char_at(0, 0), None);
        for _ in 0..3 {
            pulse(&mut lcd, false, 0x3);
        }
        pulse(&mut lcd, false, 0x2); // 4-Bit
        send(&mut lcd, false, 0x28); // 2 Zeilen
        send(&mut lcd, false, 0x0C); // Anzeige an
        send(&mut lcd, false, 0x01);
        send(&mut lcd, false, 0x06);
        for c in b"Hallo" {
            send(&mut lcd, true, *c);
        }
        send(&mut lcd, false, 0xC0);
        send(&mut lcd, true, b'X');
        let t = lcd.text();
        assert_eq!(t[0], "Hallo           ");
        assert_eq!(t[1], "X               ");
    }

    #[test]
    fn lcd_without_init_stays_dark() {
        let mut lcd = lcd_4bit();
        send(&mut lcd, true, b'A');
        assert_eq!(lcd.text()[0], "                ");
    }

    #[test]
    fn sevenseg_multiplex() {
        let mut digit = [PinRef::Nc; 8];
        digit[0] = PinRef::port(0, 0);
        digit[1] = PinRef::port(0, 1);
        let cfg = SevenSegConfig { seg: std::array::from_fn(|i| PinRef::port(1, i as u8)), digit, ..Default::default() };
        let mut s = SevenSeg::new(cfg);
        // Stelle 1 zeigt Segment a, Stelle 2 Segment b, je halbe Zeit
        s.observe(&[0x01, 0x01, 0, 0, 0], 100);
        s.observe(&[0x02, 0x02, 0, 0, 0], 100);
        let mut f = [[0.0; 8]; 8];
        s.take_frame(&[0; 5], &mut f);
        assert_eq!(f[0][0], 1.0);
        assert_eq!(f[0][1], 0.0);
        assert_eq!(f[1][1], 1.0);
    }

    #[test]
    fn funcgen_duty() {
        let g = FuncGen { pin: PinRef::port(0, 4), freq_khz: 1.0, high: 25.0, low: 75.0 };
        assert!(g.level_at(100.0));
        assert!(!g.level_at(300.0));
        assert!(g.level_at(1100.0));
    }

    #[test]
    fn stepper_counts_steps() {
        let mut s = Stepper { pins: std::array::from_fn(|i| PinRef::port(1, i as u8)), ..Default::default() };
        for p in [0x01, 0x02, 0x04, 0x08, 0x01] {
            s.observe(&[0, p, 0, 0, 0]);
        }
        assert_eq!(s.steps, 8);
        assert_eq!(s.angle, 0.0);
    }
}
