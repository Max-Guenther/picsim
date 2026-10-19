//! PIC16F84: Befehlssatz, Datenspeicher, Timer0, Watchdog, Interrupts, EEPROM.
//!
//! Maßgeblich ist das Microchip-Datenblatt DS35007. Ein Befehlszyklus dauert
//! 4 Quarztakte.

pub const PROG_SIZE: usize = 1024;
pub const EEPROM_SIZE: usize = 64;
pub const STACK_SIZE: usize = 8;

// Registeradressen
pub const INDF: u8 = 0x00;
pub const TMR0: u8 = 0x01;
pub const PCL: u8 = 0x02;
pub const STATUS: u8 = 0x03;
pub const FSR: u8 = 0x04;
pub const PORTA: u8 = 0x05;
pub const PORTB: u8 = 0x06;
pub const EEDATA: u8 = 0x08;
pub const EEADR: u8 = 0x09;
pub const PCLATH: u8 = 0x0A;
pub const INTCON: u8 = 0x0B;
pub const OPTION: u8 = 0x81;
pub const TRISA: u8 = 0x85;
pub const TRISB: u8 = 0x86;
pub const EECON1: u8 = 0x88;
pub const EECON2: u8 = 0x89;

// STATUS-Bits
pub const C: u8 = 0;
pub const DC: u8 = 1;
pub const Z: u8 = 2;
pub const PD: u8 = 3;
pub const TO: u8 = 4;
pub const RP0: u8 = 5;

// INTCON-Bits
pub const RBIF: u8 = 0;
pub const INTF: u8 = 1;
pub const T0IF: u8 = 2;
pub const RBIE: u8 = 3;
pub const INTE: u8 = 4;
pub const T0IE: u8 = 5;
pub const EEIE: u8 = 6;
pub const GIE: u8 = 7;

// OPTION-Bits
const PSA: u8 = 3;
const T0SE: u8 = 4;
const T0CS: u8 = 5;
const INTEDG: u8 = 6;

// EECON1-Bits
const EE_RD: u8 = 0;
const EE_WR: u8 = 1;
const EE_WREN: u8 = 2;
const EE_EEIF: u8 = 4;

const FLAG_Z: u8 = 1 << Z;
const FLAG_C: u8 = 1 << C;
const FLAGS_ARITH: u8 = (1 << C) | (1 << DC) | (1 << Z);

/// Nominale WDT-Zeit ohne Nachteiler laut Datenblatt.
pub const WDT_PERIOD_US: f64 = 18_000.0;
/// Dauer eines EEPROM-Schreibvorgangs.
pub const EE_WRITE_US: f64 = 10_000.0;

/// Was beim letzten Schritt Besonderes passiert ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    None,
    WatchdogReset,
    Interrupt,
    WakeUp,
}

#[derive(Debug, Clone)]
pub struct Cpu {
    pub prog: Vec<u16>,
    /// Physischer Datenspeicher. Gespiegelte Register liegen nur in Bank 0.
    ram: [u8; 256],
    pub w: u8,
    pub pc: u16,
    pub stack: [u16; STACK_SIZE],
    /// Zeigt auf den nächsten freien Stackplatz (0..7, läuft zyklisch um).
    pub sp: u8,
    /// Ausgangslatches der Ports RA..RE.
    pub latch: [u8; 5],
    /// Von außen angelegte Pegel (Schalter, Generator, Applikation, Mausklick).
    pub ext: [u8; 5],
    pub eeprom: [u8; EEPROM_SIZE],
    pub cycles: u64,
    /// Quarzfrequenz in Hz.
    pub xtal_hz: f64,
    pub wdt_enabled: bool,
    pub wdt_us: f64,
    pub sleeping: bool,
    prescaler: u8,
    tmr0_inhibit: u8,
    prev_pins: [u8; 5],
    /// Wert von PORTB beim letzten Lesen, für RB-Port-Change.
    rb_last_read: u8,
    ee_seq: u8,
    ee_write_us: Option<f64>,
    /// STATUS-Bits, die der laufende Befehl selbst setzt. Ist STATUS das
    /// Ziel, bleiben diese Bits beim Schreiben erhalten (Datenblatt 4.2.2.1).
    flag_mask: u8,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub fn new() -> Self {
        let mut cpu = Cpu {
            prog: vec![0; PROG_SIZE],
            ram: [0; 256],
            w: 0,
            pc: 0,
            stack: [0; STACK_SIZE],
            sp: 0,
            latch: [0; 5],
            ext: [0; 5],
            eeprom: [0; EEPROM_SIZE],
            cycles: 0,
            xtal_hz: 4_000_000.0,
            wdt_enabled: false,
            wdt_us: 0.0,
            sleeping: false,
            prescaler: 0,
            tmr0_inhibit: 0,
            prev_pins: [0; 5],
            rb_last_read: 0,
            ee_seq: 0,
            ee_write_us: None,
            flag_mask: 0,
        };
        cpu.power_on_reset();
        cpu
    }

    pub fn load_program(&mut self, words: &[(u16, u16)]) {
        self.prog = vec![0; PROG_SIZE];
        for &(addr, word) in words {
            if (addr as usize) < PROG_SIZE {
                self.prog[addr as usize] = word & 0x3FFF;
            }
        }
    }

    /// Dauer eines Befehlszyklus in µs.
    pub fn cycle_us(&self) -> f64 {
        4.0 / self.xtal_hz * 1e6
    }

    /// Laufzeit seit dem letzten Zurücksetzen in µs.
    pub fn runtime_us(&self) -> f64 {
        self.cycles as f64 * self.cycle_us()
    }

    pub fn power_on_reset(&mut self) {
        self.ram = [0; 256];
        self.w = 0;
        self.stack = [0; STACK_SIZE];
        self.sp = 0;
        self.cycles = 0;
        self.latch = [0; 5];
        self.reset_common();
        self.ram[STATUS as usize] = 0x18;
    }

    /// WDT-Reset: RAM und W bleiben, einige SFRs werden gesetzt, TO = 0.
    pub fn watchdog_reset(&mut self) {
        self.reset_common();
        let st = self.ram[STATUS as usize];
        self.ram[STATUS as usize] = (st & 0x07) | 0x08;
    }

    fn reset_common(&mut self) {
        self.pc = 0;
        self.ram[PCLATH as usize] = 0;
        self.ram[INTCON as usize] &= 0x01;
        self.ram[OPTION as usize] = 0xFF;
        self.ram[TRISA as usize] = 0x1F;
        self.ram[TRISB as usize] = 0xFF;
        self.ram[EECON1 as usize] &= 0x08;
        self.ram[PCL as usize] = 0;
        self.prescaler = 0;
        self.tmr0_inhibit = 0;
        self.wdt_us = 0.0;
        self.sleeping = false;
        self.ee_seq = 0;
        self.ee_write_us = None;
        self.prev_pins = self.pins();
        self.rb_last_read = self.prev_pins[1];
    }

    // ------------------------------------------------------------------
    // Datenspeicher

    /// Bildet eine Adresse (0..FF) auf die physische Speicherzelle ab.
    pub fn map(addr: u8) -> Option<u8> {
        let a = addr & 0x7F;
        let bank1 = addr & 0x80 != 0;
        match a {
            0x00 | 0x02 | 0x03 | 0x04 | 0x0A | 0x0B => Some(a),
            0x07 => None,
            0x0C..=0x4F => Some(a),
            0x50..=0x7F => None,
            _ => Some(if bank1 { a | 0x80 } else { a }),
        }
    }

    /// Lesen wie die Anzeige: ohne Seiteneffekte.
    pub fn peek(&self, addr: u8) -> u8 {
        match Self::map(addr) {
            None => 0,
            Some(PORTA) => self.pins()[0] & 0x1F,
            Some(PORTB) => self.pins()[1],
            Some(INDF) => 0,
            Some(EECON2) => 0,
            Some(a) => self.ram[a as usize],
        }
    }

    /// Schreiben aus der Oberfläche (Klick in RAM- oder SFR-Feld).
    pub fn poke(&mut self, addr: u8, value: u8) {
        match Self::map(addr) {
            None | Some(INDF) => {}
            Some(PORTA) => self.latch[0] = value & 0x1F,
            Some(PORTB) => self.latch[1] = value,
            Some(PCL) => {
                self.ram[PCL as usize] = value;
                self.pc = ((self.ram[PCLATH as usize] as u16 & 0x1F) << 8) | value as u16;
            }
            Some(a) => self.ram[a as usize] = value,
        }
    }

    pub fn status(&self) -> u8 {
        self.ram[STATUS as usize]
    }

    fn bank_addr(&self, f: u8) -> u8 {
        if f == INDF {
            return self.ram[FSR as usize];
        }
        let rp0 = (self.ram[STATUS as usize] >> RP0) & 1;
        (rp0 << 7) | (f & 0x7F)
    }

    fn read_f(&mut self, f: u8) -> u8 {
        let addr = self.bank_addr(f);
        match Self::map(addr) {
            Some(PORTB) => {
                let v = self.pins()[1];
                self.rb_last_read = v;
                v
            }
            _ => self.peek(addr),
        }
    }

    /// Schreibt ein Register so, wie es ein Befehl tut.
    fn write_f(&mut self, f: u8, value: u8) {
        let addr = self.bank_addr(f);
        let Some(phys) = Self::map(addr) else { return };
        match phys {
            INDF => {}
            TMR0 => {
                self.ram[TMR0 as usize] = value;
                self.tmr0_inhibit = 2;
                if self.option_bit(PSA) == 0 {
                    self.prescaler = 0;
                }
            }
            PORTA => self.latch[0] = value & 0x1F,
            PORTB => self.latch[1] = value,
            STATUS => {
                // TO und PD sind nur lesbar, vom Befehl gesetzte Flags gewinnen
                let keep = 0x18 | self.flag_mask;
                let old = self.ram[STATUS as usize];
                self.ram[STATUS as usize] = (value & !keep) | (old & keep);
            }
            EECON1 => {
                let old = self.ram[EECON1 as usize];
                // RD und WR lassen sich nur setzen, nicht per Software löschen
                let mut v = (value & 0x1C) | (old & ((1 << EE_RD) | (1 << EE_WR)));
                if value & (1 << EE_RD) != 0 {
                    let a = self.ram[EEADR as usize] as usize % EEPROM_SIZE;
                    self.ram[EEDATA as usize] = self.eeprom[a];
                    v &= !(1 << EE_RD);
                }
                if value & (1 << EE_WR) != 0
                    && old & (1 << EE_WR) == 0
                    && v & (1 << EE_WREN) != 0
                    && self.ee_seq == 2
                {
                    v |= 1 << EE_WR;
                    self.ee_write_us = Some(EE_WRITE_US);
                }
                self.ee_seq = 0;
                self.ram[EECON1 as usize] = v;
            }
            EECON2 => {
                self.ee_seq = match (self.ee_seq, value) {
                    (_, 0x55) => 1,
                    (1, 0xAA) => 2,
                    _ => 0,
                };
            }
            a => self.ram[a as usize] = value,
        }
    }

    fn option_bit(&self, bit: u8) -> u8 {
        (self.ram[OPTION as usize] >> bit) & 1
    }

    fn set_status_bit(&mut self, bit: u8, on: bool) {
        let s = &mut self.ram[STATUS as usize];
        if on {
            *s |= 1 << bit;
        } else {
            *s &= !(1 << bit);
        }
    }

    fn set_z(&mut self, v: u8) {
        self.set_status_bit(Z, v == 0);
    }

    // ------------------------------------------------------------------
    // Ports

    pub fn tris(&self, port: usize) -> u8 {
        match port {
            0 => self.ram[TRISA as usize] | 0xE0,
            1 => self.ram[TRISB as usize],
            _ => 0xFF,
        }
    }

    /// Anzahl der Pins eines Ports beim 16F84.
    pub fn port_width(port: usize) -> u8 {
        match port {
            0 => 5,
            1 => 8,
            _ => 0,
        }
    }

    /// Aktuelle Pegel aller Pins: Ausgänge aus dem Latch, Eingänge von außen.
    pub fn pins(&self) -> [u8; 5] {
        let mut p = [0u8; 5];
        for (i, v) in p.iter_mut().enumerate() {
            let t = self.tris(i);
            *v = (self.latch[i] & !t) | (self.ext[i] & t);
        }
        p[0] &= 0x1F;
        p
    }

    pub fn pin(&self, port: u8, bit: u8) -> bool {
        self.pins()[port as usize] >> bit & 1 != 0
    }

    pub fn set_input(&mut self, port: u8, bit: u8, level: bool) {
        let e = &mut self.ext[port as usize];
        if level {
            *e |= 1 << bit;
        } else {
            *e &= !(1 << bit);
        }
    }

    /// Klick auf einen Pin in der Portanzeige: schaltet den Pegel um,
    /// unabhängig vom TRIS-Register (wie im Original).
    pub fn toggle_pin(&mut self, port: u8, bit: u8) {
        let lvl = self.pin(port, bit);
        self.set_input(port, bit, !lvl);
        if self.tris(port as usize) >> bit & 1 == 0 {
            self.latch[port as usize] ^= 1 << bit;
        }
    }

    /// Prüft Flanken an RA4/T0CKI, RB0/INT und RB4..7.
    /// Läuft vor jedem Befehl, damit externe Änderungen ankommen.
    fn check_pin_events(&mut self) {
        let now = self.pins();
        let prev = self.prev_pins;
        self.prev_pins = now;

        if self.option_bit(T0CS) == 1 {
            let was = prev[0] >> 4 & 1;
            let is = now[0] >> 4 & 1;
            let edge = if self.option_bit(T0SE) == 0 {
                was == 0 && is == 1
            } else {
                was == 1 && is == 0
            };
            if edge {
                self.tmr0_tick();
            }
        }

        let was = prev[1] & 1;
        let is = now[1] & 1;
        let edge = if self.option_bit(INTEDG) == 1 {
            was == 0 && is == 1
        } else {
            was == 1 && is == 0
        };
        if edge && self.ram[TRISB as usize] & 1 != 0 {
            self.ram[INTCON as usize] |= 1 << INTF;
        }

        let mask = self.ram[TRISB as usize] & 0xF0;
        if (now[1] ^ self.rb_last_read) & mask != 0 {
            self.ram[INTCON as usize] |= 1 << RBIF;
        }
    }

    // ------------------------------------------------------------------
    // Timer0 und Watchdog

    fn tmr0_tick(&mut self) {
        if self.tmr0_inhibit > 0 {
            return;
        }
        if self.option_bit(PSA) == 0 {
            let ratio = 2u16 << (self.ram[OPTION as usize] & 0x07);
            self.prescaler = self.prescaler.wrapping_add(1);
            if (self.prescaler as u16) < ratio {
                return;
            }
            self.prescaler = 0;
        }
        let t = self.ram[TMR0 as usize].wrapping_add(1);
        self.ram[TMR0 as usize] = t;
        if t == 0 {
            self.ram[INTCON as usize] |= 1 << T0IF;
        }
    }

    /// WDT-Zeit inklusive Nachteiler.
    pub fn wdt_timeout_us(&self) -> f64 {
        if self.option_bit(PSA) == 1 {
            WDT_PERIOD_US * (1u32 << (self.ram[OPTION as usize] & 0x07)) as f64
        } else {
            WDT_PERIOD_US
        }
    }

    fn clear_wdt(&mut self) {
        self.wdt_us = 0.0;
        if self.option_bit(PSA) == 1 {
            self.prescaler = 0;
        }
    }

    /// Lässt n Befehlszyklen verstreichen.
    fn advance(&mut self, n: u32) -> Event {
        let mut ev = Event::None;
        for _ in 0..n {
            self.cycles += 1;
            if self.tmr0_inhibit > 0 {
                self.tmr0_inhibit -= 1;
            } else if self.option_bit(T0CS) == 0 && !self.sleeping {
                self.tmr0_tick();
            }
        }
        let dt = n as f64 * self.cycle_us();

        if let Some(rest) = self.ee_write_us {
            let rest = rest - dt;
            if rest <= 0.0 {
                let a = self.ram[EEADR as usize] as usize % EEPROM_SIZE;
                self.eeprom[a] = self.ram[EEDATA as usize];
                self.ram[EECON1 as usize] &= !(1 << EE_WR);
                self.ram[EECON1 as usize] |= 1 << EE_EEIF;
                self.ee_write_us = None;
            } else {
                self.ee_write_us = Some(rest);
            }
        }

        if self.wdt_enabled {
            self.wdt_us += dt;
            if self.wdt_us >= self.wdt_timeout_us() {
                if self.sleeping {
                    self.wdt_us = 0.0;
                    self.sleeping = false;
                    self.set_status_bit(TO, false);
                    self.set_status_bit(PD, false);
                    ev = Event::WakeUp;
                } else {
                    self.watchdog_reset();
                    ev = Event::WatchdogReset;
                }
            }
        }
        ev
    }

    // ------------------------------------------------------------------
    // Stack

    fn push(&mut self, addr: u16) {
        self.stack[self.sp as usize] = addr;
        self.sp = (self.sp + 1) % STACK_SIZE as u8;
    }

    fn pop(&mut self) -> u16 {
        self.sp = (self.sp + STACK_SIZE as u8 - 1) % STACK_SIZE as u8;
        self.stack[self.sp as usize]
    }

    // ------------------------------------------------------------------
    // Ausführung

    fn interrupt_pending(&self) -> bool {
        let ic = self.ram[INTCON as usize];
        let ee = self.ram[EECON1 as usize] >> EE_EEIF & 1 != 0 && ic >> EEIE & 1 != 0;
        (ic & 0x07) & ((ic >> 3) & 0x07) != 0 || ee
    }

    fn set_pc(&mut self, pc: u16) {
        self.pc = pc & 0x1FFF;
        self.ram[PCL as usize] = self.pc as u8;
    }

    /// Überspringt den nächsten Befehl ohne ihn auszuführen (Ignore, F7).
    pub fn ignore(&mut self) {
        self.set_pc(self.pc.wrapping_add(1));
    }

    /// Führt einen Befehl aus.
    pub fn step(&mut self) -> Event {
        self.check_pin_events();

        if self.sleeping {
            if self.interrupt_pending() {
                self.sleeping = false;
                self.set_status_bit(PD, false);
                return Event::WakeUp;
            }
            return self.advance(1);
        }

        if self.ram[INTCON as usize] >> GIE & 1 != 0 && self.interrupt_pending() {
            self.ram[INTCON as usize] &= !(1 << GIE);
            self.push(self.pc);
            self.set_pc(0x0004);
            self.advance(1);
            return Event::Interrupt;
        }

        let op = self.prog[(self.pc as usize) % PROG_SIZE];
        self.set_pc(self.pc.wrapping_add(1));
        self.flag_mask = 0;
        let cyc = self.execute(op);
        self.advance(cyc)
    }

    /// Dekodiert und führt einen 14-Bit-Befehl aus. Liefert die Zyklenzahl.
    fn execute(&mut self, op: u16) -> u32 {
        let f = (op & 0x7F) as u8;
        let d = op & 0x80 != 0;
        let k = (op & 0xFF) as u8;
        let b = ((op >> 7) & 0x07) as u8;

        match op >> 12 {
            0b00 => match (op >> 8) & 0x0F {
                0b0000 => {
                    if d {
                        // MOVWF
                        self.store(f, true, self.w)
                    } else {
                        match op {
                            0x0008 => {
                                // RETURN
                                let a = self.pop();
                                self.set_pc(a);
                                2
                            }
                            0x0009 => {
                                // RETFIE
                                let a = self.pop();
                                self.set_pc(a);
                                self.ram[INTCON as usize] |= 1 << GIE;
                                2
                            }
                            0x0063 => {
                                // SLEEP
                                self.clear_wdt();
                                self.set_status_bit(TO, true);
                                self.set_status_bit(PD, false);
                                self.sleeping = true;
                                1
                            }
                            0x0064 => {
                                // CLRWDT
                                self.clear_wdt();
                                self.set_status_bit(TO, true);
                                self.set_status_bit(PD, true);
                                1
                            }
                            _ => 1, // NOP
                        }
                    }
                }
                0b0001 => {
                    // CLRF / CLRW
                    self.flag_mask = FLAG_Z;
                    self.set_status_bit(Z, true);
                    self.store(f, d, 0)
                }
                0b0010 => {
                    // SUBWF
                    let a = self.read_f(f);
                    let r = self.sub(a, self.w);
                    self.store(f, d, r)
                }
                0b0011 => self.unary(f, d, |v| v.wrapping_sub(1)),
                0b0100 => self.logic(f, d, |a, w| a | w),
                0b0101 => self.logic(f, d, |a, w| a & w),
                0b0110 => self.logic(f, d, |a, w| a ^ w),
                0b0111 => {
                    // ADDWF
                    let a = self.read_f(f);
                    let r = self.add(a, self.w);
                    self.store(f, d, r)
                }
                0b1000 => self.unary(f, d, |v| v),
                0b1001 => self.unary(f, d, |v| !v),
                0b1010 => self.unary(f, d, |v| v.wrapping_add(1)),
                0b1011 => self.skip_if_zero(f, d, |v| v.wrapping_sub(1)),
                0b1100 => {
                    // RRF
                    let v = self.read_f(f);
                    let c = self.status() & 1;
                    self.flag_mask = FLAG_C;
                    self.set_status_bit(C, v & 1 != 0);
                    self.store(f, d, (v >> 1) | (c << 7))
                }
                0b1101 => {
                    // RLF
                    let v = self.read_f(f);
                    let c = self.status() & 1;
                    self.flag_mask = FLAG_C;
                    self.set_status_bit(C, v & 0x80 != 0);
                    self.store(f, d, (v << 1) | c)
                }
                0b1110 => {
                    // SWAPF
                    let v = self.read_f(f);
                    self.store(f, d, v.rotate_left(4))
                }
                _ => self.skip_if_zero(f, d, |v| v.wrapping_add(1)),
            },
            0b01 => {
                let v = self.read_f(f);
                match (op >> 10) & 0x03 {
                    0b00 => self.store(f, true, v & !(1 << b)),
                    0b01 => self.store(f, true, v | (1 << b)),
                    0b10 => self.skip(v >> b & 1 == 0),
                    _ => self.skip(v >> b & 1 != 0),
                }
            }
            0b10 => {
                // CALL / GOTO
                let target = (op & 0x07FF) | ((self.ram[PCLATH as usize] as u16 & 0x18) << 8);
                if op & 0x0800 == 0 {
                    self.push(self.pc);
                }
                self.set_pc(target);
                2
            }
            _ => match (op >> 8) & 0x0F {
                0b0000..=0b0011 => {
                    // MOVLW
                    self.w = k;
                    1
                }
                0b0100..=0b0111 => {
                    // RETLW
                    self.w = k;
                    let a = self.pop();
                    self.set_pc(a);
                    2
                }
                0b1000 => {
                    self.w |= k;
                    self.set_z(self.w);
                    1
                }
                0b1001 => {
                    self.w &= k;
                    self.set_z(self.w);
                    1
                }
                0b1010 => {
                    self.w ^= k;
                    self.set_z(self.w);
                    1
                }
                0b1100 | 0b1101 => {
                    // SUBLW: k - W
                    self.w = self.sub(k, self.w);
                    1
                }
                0b1110 | 0b1111 => {
                    self.w = self.add(k, self.w);
                    1
                }
                _ => 1, // 0b1011 ist nicht belegt
            },
        }
    }

    /// Schreibt das Ergebnis nach W (d=0) oder f (d=1). Liefert Zyklen.
    fn store(&mut self, f: u8, d: bool, v: u8) -> u32 {
        if !d {
            self.w = v;
            return 1;
        }
        let target = Self::map(self.bank_addr(f));
        self.write_f(f, v);
        if target == Some(PCL) {
            // Computed goto: PC<12:8> kommt aus PCLATH, kostet 2 Zyklen
            let pclath = self.ram[PCLATH as usize] as u16 & 0x1F;
            self.set_pc((pclath << 8) | v as u16);
            return 2;
        }
        1
    }

    fn unary(&mut self, f: u8, d: bool, op: impl Fn(u8) -> u8) -> u32 {
        let v = op(self.read_f(f));
        self.flag_mask = FLAG_Z;
        self.set_z(v);
        self.store(f, d, v)
    }

    fn logic(&mut self, f: u8, d: bool, op: impl Fn(u8, u8) -> u8) -> u32 {
        let v = op(self.read_f(f), self.w);
        self.flag_mask = FLAG_Z;
        self.set_z(v);
        self.store(f, d, v)
    }

    fn skip_if_zero(&mut self, f: u8, d: bool, op: impl Fn(u8) -> u8) -> u32 {
        let v = op(self.read_f(f));
        let cyc = self.store(f, d, v);
        if v == 0 {
            self.skip(true)
        } else {
            cyc
        }
    }

    fn skip(&mut self, cond: bool) -> u32 {
        if cond {
            self.set_pc(self.pc.wrapping_add(1));
            2
        } else {
            1
        }
    }

    fn add(&mut self, a: u8, b: u8) -> u8 {
        let r = a as u16 + b as u16;
        self.flag_mask = FLAGS_ARITH;
        self.set_status_bit(C, r > 0xFF);
        self.set_status_bit(DC, (a & 0x0F) + (b & 0x0F) > 0x0F);
        let r = r as u8;
        self.set_z(r);
        r
    }

    /// a - b. C und DC bedeuten "kein Borgen".
    fn sub(&mut self, a: u8, b: u8) -> u8 {
        let r = a.wrapping_sub(b);
        self.flag_mask = FLAGS_ARITH;
        self.set_status_bit(C, a >= b);
        self.set_status_bit(DC, (a & 0x0F) >= (b & 0x0F));
        self.set_z(r);
        r
    }

    pub fn current_op(&self) -> u16 {
        self.prog[self.pc as usize % PROG_SIZE]
    }
}

pub fn is_call(op: u16) -> bool {
    op & 0x3800 == 0x2000
}

pub fn is_return(op: u16) -> bool {
    op == 0x0008 || op == 0x0009 || op & 0x3C00 == 0x3400
}
