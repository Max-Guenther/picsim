//! Das simulierte Board: CPU, Programm, Breakpoints und angeschlossene
//! Peripherie. Die Oberfläche spricht nur mit dem Board.

use crate::cpu::{is_call, is_return, Cpu, Event};
use crate::devices::{level, FuncGen, Lcd, LcdConfig, SevenSeg, SevenSegConfig, Stepper};
use crate::ini::Ini;
use crate::lst::Listing;
use crate::pins::PinRef;
use std::collections::BTreeSet;

/// Warum ein Lauf angehalten hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Schritt bzw. Step over/out fertig.
    Done,
    Breakpoint,
    Watchdog,
    /// Schrittbudget aufgebraucht, Lauf geht weiter.
    Budget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Stopped,
    Go,
    StepOver { target: u16, sp: u8 },
    StepOut { depth: i32 },
}

/// Stellung eines Schiebeschalters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwitchPos {
    High,
    #[default]
    Open,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedLayout {
    #[default]
    Wuerfel,
    Leuchtband,
}

pub const LED_COLORS: [&str; 5] = ["rot", "gruen", "gelb", "blau", "weiss"];
pub const KEY_NAMES: [&str; 3] = ["S51", "S52", "S33"];

#[derive(Debug, Clone)]
pub struct Board {
    pub cpu: Cpu,
    pub listing: Listing,
    pub breakpoints: BTreeSet<u16>,
    pub mode: RunMode,
    pub ini: Ini,

    pub lcd: Lcd,
    pub sseg: SevenSeg,
    pub fgen: FuncGen,
    pub stepper: Stepper,

    pub leds: [PinRef; 9],
    pub led_colors: [usize; 9],
    pub led_layout: LedLayout,
    pub switches: [PinRef; 16],
    pub switch_names: [String; 16],
    pub switch_pos: [SwitchPos; 16],
    pub keys: [PinRef; 3],
    pub key_latching: [bool; 3],
    pub key_pressed: [bool; 3],
    skip_breakpoint: bool,
}

impl Default for Board {
    fn default() -> Self {
        Self::new(Ini::default())
    }
}

impl Board {
    pub fn new(ini: Ini) -> Self {
        let mut b = Board {
            cpu: Cpu::new(),
            listing: Listing::default(),
            breakpoints: BTreeSet::new(),
            mode: RunMode::Stopped,
            ini: Ini::default(),
            lcd: Lcd::new(LcdConfig::default()),
            sseg: SevenSeg::default(),
            fgen: FuncGen::default(),
            stepper: Stepper::default(),
            leds: [PinRef::Nc; 9],
            led_colors: [0; 9],
            led_layout: LedLayout::Wuerfel,
            switches: [PinRef::Nc; 16],
            switch_names: Default::default(),
            switch_pos: [SwitchPos::Open; 16],
            keys: [PinRef::Nc; 3],
            key_latching: [true; 3],
            key_pressed: [false; 3],
            skip_breakpoint: false,
        };
        b.apply_ini(ini);
        b
    }

    /// Übernimmt alle Einstellungen aus der INI.
    pub fn apply_ini(&mut self, ini: Ini) {
        self.cpu.xtal_hz = ini.xtal_hz();
        self.lcd.cfg = LcdConfig::from_ini(&ini);
        self.sseg = SevenSeg::new(SevenSegConfig::from_ini(&ini));
        self.fgen = FuncGen::from_ini(&ini, "FGEN_0");
        let steps = self.stepper.steps;
        self.stepper = Stepper::from_ini(&ini);
        self.stepper.steps = steps;
        for i in 0..9 {
            self.leds[i] = ini.pin("LED_ARRAY", &format!("LED{i}"));
            self.led_colors[i] = ini
                .get("LED_ARRAY", &format!("COLOR{i}"))
                .and_then(|c| LED_COLORS.iter().position(|n| n.eq_ignore_ascii_case(c)))
                .unwrap_or(0);
        }
        self.led_layout = if ini.flag("LED_ARRAY", "DIR2") { LedLayout::Leuchtband } else { LedLayout::Wuerfel };
        for i in 0..16 {
            self.switches[i] = ini.pin("SWITCH", &format!("S{i}"));
            let name = ini.get("SWITCH", &format!("N{i}")).unwrap_or("");
            self.switch_names[i] = if name == "nc" { String::new() } else { name.to_string() };
        }
        for (i, k) in KEY_NAMES.iter().enumerate() {
            self.keys[i] = ini.pin("KEYS", k);
            self.key_latching[i] = ini.get("KEYLOCK", k).map(|v| v == "1").unwrap_or(true);
        }
        self.ini = ini;
        self.drive_inputs();
    }

    /// Schreibt die aktuellen Einstellungen in die INI zurück.
    pub fn store_ini(&mut self) {
        let mut ini = std::mem::take(&mut self.ini);
        ini.set_xtal_hz(self.cpu.xtal_hz);
        self.lcd.cfg.to_ini(&mut ini);
        self.sseg.cfg.to_ini(&mut ini);
        self.fgen.to_ini(&mut ini, "FGEN_0");
        self.stepper.to_ini(&mut ini);
        for i in 0..9 {
            ini.set_pin("LED_ARRAY", &format!("LED{i}"), self.leds[i]);
            ini.set("LED_ARRAY", &format!("COLOR{i}"), LED_COLORS[self.led_colors[i]]);
        }
        ini.set_flag("LED_ARRAY", "DIR1", self.led_layout == LedLayout::Wuerfel);
        ini.set_flag("LED_ARRAY", "DIR2", self.led_layout == LedLayout::Leuchtband);
        for i in 0..16 {
            ini.set_pin("SWITCH", &format!("S{i}"), self.switches[i]);
            let n = if self.switch_names[i].is_empty() { "nc" } else { &self.switch_names[i] };
            ini.set("SWITCH", &format!("N{i}"), n);
        }
        for (i, k) in KEY_NAMES.iter().enumerate() {
            ini.set_pin("KEYS", k, self.keys[i]);
            ini.set_flag("KEYLOCK", k, self.key_latching[i]);
        }
        self.ini = ini;
    }

    pub fn load_listing(&mut self, listing: Listing) {
        self.cpu.load_program(&listing.program());
        if let Some(w) = listing.wdt {
            self.cpu.wdt_enabled = w;
        }
        // Breakpoints auf Adressen behalten, die es noch gibt
        let prog: BTreeSet<u16> = listing.program().iter().map(|p| p.0).collect();
        self.breakpoints.retain(|a| prog.contains(a));
        self.listing = listing;
        self.reset();
    }

    /// Power-On-Reset (F2). Bricht auch einen laufenden Go ab.
    pub fn reset(&mut self) {
        self.mode = RunMode::Stopped;
        self.cpu.power_on_reset();
        self.lcd.power_on();
        self.sseg.reset();
        self.drive_inputs();
    }

    /// Setzt alle Eingänge, die von Schaltern und Tastern kommen.
    pub fn drive_inputs(&mut self) {
        for i in 0..16 {
            if let PinRef::Port { port, bit } = self.switches[i] {
                match self.switch_pos[i] {
                    SwitchPos::High => self.cpu.set_input(port, bit, true),
                    SwitchPos::Low => self.cpu.set_input(port, bit, false),
                    SwitchPos::Open => {}
                }
            }
        }
        for i in 0..3 {
            if let PinRef::Port { port, bit } = self.keys[i] {
                self.cpu.set_input(port, bit, self.key_pressed[i]);
            }
        }
    }

    pub fn set_switch(&mut self, i: usize, pos: SwitchPos) {
        self.switch_pos[i] = pos;
        self.drive_inputs();
    }

    pub fn press_key(&mut self, i: usize, pressed: bool) {
        self.key_pressed[i] = pressed;
        self.drive_inputs();
    }

    pub fn led_on(&self, i: usize) -> bool {
        level(&self.cpu.pins(), self.leds[i]).unwrap_or(false)
    }

    pub fn toggle_breakpoint(&mut self, addr: u16) {
        if !self.breakpoints.remove(&addr) {
            self.breakpoints.insert(addr);
        }
    }

    // ------------------------------------------------------------------
    // Ausführung

    fn step_once(&mut self) -> Event {
        if let PinRef::Port { port, bit } = self.fgen.pin {
            let l = self.fgen.level_at(self.cpu.runtime_us());
            self.cpu.set_input(port, bit, l);
        }
        let before = self.cpu.cycles;
        let ev = self.cpu.step();
        let pins = self.cpu.pins();
        self.lcd.observe(&pins);
        self.sseg.observe(&pins, self.cpu.cycles.saturating_sub(before) as u32);
        self.stepper.observe(&pins);
        ev
    }

    /// Step in (F8).
    pub fn step_in(&mut self) -> Stop {
        self.mode = RunMode::Stopped;
        match self.step_once() {
            Event::WatchdogReset => Stop::Watchdog,
            _ => Stop::Done,
        }
    }

    /// Ignore (F7): nächsten Befehl überspringen.
    pub fn ignore(&mut self) {
        self.mode = RunMode::Stopped;
        self.cpu.ignore();
    }

    /// Step over (F9): Unterprogramm in einem Rutsch ausführen.
    /// Liefert `Budget`, wenn danach `run` weiterlaufen muss. Das
    /// Unterprogramm läuft dann wirklich Befehl für Befehl (mit Timer,
    /// Interrupts und Breakpoints) in den Blöcken von `run`, bis es an die
    /// Adresse hinter dem CALL mit derselben Stacktiefe zurückkehrt.
    pub fn step_over(&mut self) -> Stop {
        if is_call(self.cpu.current_op()) {
            // Wie set_pc: der PC hat 13 Bit, sonst würde das Ziel hinter
            // 1FFFh nie erreicht und Step over liefe endlos
            let target = self.cpu.pc.wrapping_add(1) & 0x1FFF;
            self.start(RunMode::StepOver { target, sp: self.cpu.sp });
            Stop::Budget
        } else {
            self.step_in()
        }
    }

    /// Step out (F6): laufen, bis das aktuelle Unterprogramm zurückkehrt.
    pub fn step_out(&mut self) {
        self.start(RunMode::StepOut { depth: 0 });
    }

    /// Go (F10). Ein zweiter Aufruf hält an.
    pub fn go(&mut self) {
        if self.mode == RunMode::Stopped {
            self.start(RunMode::Go);
        } else {
            self.mode = RunMode::Stopped;
        }
    }

    pub fn running(&self) -> bool {
        self.mode != RunMode::Stopped
    }

    fn start(&mut self, mode: RunMode) {
        self.mode = mode;
        self.skip_breakpoint = true;
    }

    /// Führt bis zu `max_steps` Befehle im aktuellen Modus aus.
    pub fn run(&mut self, max_steps: usize) -> Stop {
        for _ in 0..max_steps {
            if self.mode == RunMode::Stopped {
                return Stop::Done;
            }
            if !self.skip_breakpoint && self.breakpoints.contains(&self.cpu.pc) {
                self.mode = RunMode::Stopped;
                return Stop::Breakpoint;
            }
            self.skip_breakpoint = false;

            let op = self.cpu.current_op();
            let ev = self.step_once();
            if ev == Event::WatchdogReset {
                self.mode = RunMode::Stopped;
                return Stop::Watchdog;
            }
            match &mut self.mode {
                RunMode::StepOver { target, sp } => {
                    if self.cpu.pc == *target && self.cpu.sp == *sp {
                        self.mode = RunMode::Stopped;
                        return Stop::Done;
                    }
                }
                RunMode::StepOut { depth } => {
                    if ev == Event::Interrupt || is_call(op) {
                        *depth += 1;
                    } else if is_return(op) {
                        if *depth == 0 {
                            self.mode = RunMode::Stopped;
                            return Stop::Done;
                        }
                        *depth -= 1;
                    }
                }
                _ => {}
            }
        }
        Stop::Budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(code: &[u16]) -> Board {
        let mut b = Board::default();
        let words: Vec<_> = code.iter().enumerate().map(|(i, &w)| (i as u16, w)).collect();
        b.cpu.load_program(&words);
        b
    }

    const NOP: u16 = 0x0000;
    const RETURN: u16 = 0x0008;
    fn call(k: u16) -> u16 {
        0x2000 | k
    }
    fn goto(k: u16) -> u16 {
        0x2800 | k
    }

    #[test]
    fn go_stops_at_breakpoint() {
        let mut b = board(&[NOP, NOP, NOP, goto(0)]);
        b.toggle_breakpoint(2);
        b.go();
        assert_eq!(b.run(100), Stop::Breakpoint);
        assert_eq!(b.cpu.pc, 2);
        // Weiterlaufen startet nicht sofort wieder am selben Breakpoint
        b.go();
        assert_eq!(b.run(4), Stop::Budget);
    }

    #[test]
    fn step_over_runs_subroutine() {
        let mut b = board(&[call(3), NOP, goto(1), NOP, NOP, RETURN]);
        assert_eq!(b.step_over(), Stop::Budget);
        assert_eq!(b.run(100), Stop::Done);
        assert_eq!(b.cpu.pc, 1);
    }

    /// Unterprogramm mit gut 20.000 Befehlen: Step over muss es ganz
    /// ausführen, darf dabei nicht in einem einzigen `run`-Block hängen und
    /// hält genau an der Folgeinstruktion des CALL.
    #[test]
    fn step_over_long_subroutine_stops_after_call() {
        const INCF_0E: u16 = 0x0A8E; // incf 0x0e,f
        const DECFSZ_0D: u16 = 0x0B8D; // decfsz 0x0d,f
        const DECFSZ_0C: u16 = 0x0B8C; // decfsz 0x0c,f
        let code = [
            call(4),     // 0: CALL lang
            NOP,         // 1: Folgeinstruktion, hier muss Step over halten
            NOP,         // 2
            goto(2),     // 3
            0x3028,      // 4: lang  movlw 28h (40)
            0x008C,      //  5:      movwf 0x0c
            0x30FF,      //  6: aussen movlw FFh (255)
            0x008D,      //  7:      movwf 0x0d
            INCF_0E,     //  8: innen incf 0x0e,f  (zählt mit)
            DECFSZ_0D,   //  9:      decfsz 0x0d,f
            goto(8),     // 10:      goto innen
            DECFSZ_0C,   // 11:      decfsz 0x0c,f
            goto(6),     // 12:      goto aussen
            RETURN,      // 13
        ];
        let mut b = board(&code);
        assert_eq!(b.step_over(), Stop::Budget, "CALL startet einen Lauf");
        assert!(b.running());

        let mut blocks = 0;
        let executed_at_stop;
        loop {
            blocks += 1;
            assert!(blocks < 10_000, "Step over endet nicht");
            let before = b.cpu.cycles;
            let stop = b.run(97);
            if stop != Stop::Budget {
                assert_eq!(stop, Stop::Done);
                executed_at_stop = b.cpu.cycles - before;
                break;
            }
            // Zwischendurch steht der PC im Unterprogramm, der Lauf geht weiter
            assert!(b.running());
            assert!((4..=13).contains(&b.cpu.pc), "PC {:04X} außerhalb des Unterprogramms", b.cpu.pc);
        }
        assert!(executed_at_stop > 0);
        assert!(blocks > 100, "in Blöcken ausgeführt, nicht auf einmal ({blocks})");
        assert!(!b.running());
        assert_eq!(b.cpu.pc, 1, "genau an der Folgeinstruktion");
        assert_eq!(b.cpu.sp, 0, "Rücksprung vom Stack genommen");
        // 40 * 255 Durchläufe der inneren Schleife wurden wirklich ausgeführt
        assert_eq!(b.cpu.peek(0x0E), (40u32 * 255 % 256) as u8);
        assert!(b.cpu.cycles > 20_000, "nur {} Zyklen", b.cpu.cycles);
        assert_eq!(b.cpu.peek(0x0C), 0);

        // Der nächste Step over auf einem normalen Befehl ist ein einzelner Schritt
        assert_eq!(b.step_over(), Stop::Done);
        assert_eq!(b.cpu.pc, 2);
    }

    #[test]
    fn step_over_recursive_call_waits_for_own_return() {
        // Unterprogramm ruft sich einmal selbst auf. Halten darf erst der
        // äußere Rücksprung (gleiche Stacktiefe wie vor dem CALL).
        // 0: call 3 / 1: nop / 2: goto 1 / 3: btfss 0x20,0 / 4: goto 6
        // 5: return / 6: bsf 0x20,0 / 7: call 3 / 8: return
        let code = [call(3), NOP, goto(1), 0x1C20, goto(6), RETURN, 0x1420, call(3), RETURN];
        let mut b = board(&code);
        assert_eq!(b.step_over(), Stop::Budget);
        assert_eq!(b.run(1000), Stop::Done);
        assert_eq!(b.cpu.pc, 1);
        assert_eq!(b.cpu.sp, 0);
    }

    #[test]
    fn step_over_stops_at_breakpoint_inside_subroutine() {
        let mut b = board(&[call(3), NOP, goto(1), NOP, NOP, RETURN]);
        b.toggle_breakpoint(4);
        assert_eq!(b.step_over(), Stop::Budget);
        assert_eq!(b.run(100), Stop::Breakpoint);
        assert_eq!(b.cpu.pc, 4);
    }

    #[test]
    fn step_over_call_at_end_of_pc_range() {
        let mut b = board(&[]);
        // CALL ganz oben im 13-Bit-Adressraum, Rücksprung auf 0000
        b.cpu.prog[0x3FF] = call(0x10);
        b.cpu.prog[0x10] = RETURN;
        b.cpu.pc = 0x1FFF; // spiegelt auf 0x3FF im 1K-Programmspeicher
        assert_eq!(b.step_over(), Stop::Budget);
        assert_eq!(b.run(100), Stop::Done, "Ziel hinter 1FFFh wird erkannt");
        assert_eq!(b.cpu.pc, 0);
    }

    #[test]
    fn step_out_returns_from_nested() {
        let mut b = board(&[call(2), goto(0), NOP, call(5), RETURN, NOP, RETURN]);
        b.step_in(); // jetzt im Unterprogramm bei 2
        b.step_out();
        assert_eq!(b.run(100), Stop::Done);
        assert_eq!(b.cpu.pc, 1);
    }

    #[test]
    fn switches_drive_inputs() {
        let mut b = Board::default();
        b.switches[0] = PinRef::port(1, 3);
        b.set_switch(0, SwitchPos::High);
        assert!(b.cpu.pin(1, 3));
        b.set_switch(0, SwitchPos::Low);
        assert!(!b.cpu.pin(1, 3));
    }

    #[test]
    fn ini_roundtrip_of_settings() {
        let ini = Ini::parse(include_str!("../tests/picsimu.ini"));
        let mut b = Board::new(ini);
        assert_eq!(b.leds[0], PinRef::port(1, 0));
        b.store_ini();
        let b2 = Board::new(b.ini.clone());
        assert_eq!(b2.leds, b.leds);
        assert_eq!(b2.lcd.cfg.size(), (2, 16));
    }
}
