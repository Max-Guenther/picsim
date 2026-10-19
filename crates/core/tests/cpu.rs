use picsim_core::cpu::*;
use picsim_core::{Cpu, Event, Listing};

// Opcode-Bausteine laut Datenblatt
fn movlw(k: u8) -> u16 { 0x3000 | k as u16 }
fn addlw(k: u8) -> u16 { 0x3E00 | k as u16 }
fn sublw(k: u8) -> u16 { 0x3C00 | k as u16 }
fn movwf(f: u8) -> u16 { 0x0080 | f as u16 }
fn movf(f: u8, d: bool) -> u16 { 0x0800 | (d as u16) << 7 | f as u16 }
fn addwf(f: u8, d: bool) -> u16 { 0x0700 | (d as u16) << 7 | f as u16 }
fn subwf(f: u8, d: bool) -> u16 { 0x0200 | (d as u16) << 7 | f as u16 }
fn decfsz(f: u8, d: bool) -> u16 { 0x0B00 | (d as u16) << 7 | f as u16 }
fn rlf(f: u8, d: bool) -> u16 { 0x0D00 | (d as u16) << 7 | f as u16 }
fn rrf(f: u8, d: bool) -> u16 { 0x0C00 | (d as u16) << 7 | f as u16 }
fn swapf(f: u8, d: bool) -> u16 { 0x0E00 | (d as u16) << 7 | f as u16 }
fn clrf(f: u8) -> u16 { 0x0180 | f as u16 }
fn bsf(f: u8, b: u8) -> u16 { 0x1400 | (b as u16) << 7 | f as u16 }
fn bcf(f: u8, b: u8) -> u16 { 0x1000 | (b as u16) << 7 | f as u16 }
fn btfss(f: u8, b: u8) -> u16 { 0x1C00 | (b as u16) << 7 | f as u16 }
fn goto(k: u16) -> u16 { 0x2800 | k }
fn call(k: u16) -> u16 { 0x2000 | k }
fn retlw(k: u8) -> u16 { 0x3400 | k as u16 }
const RETFIE: u16 = 0x0009;
const NOP: u16 = 0x0000;
const CLRWDT: u16 = 0x0064;
const SLEEP: u16 = 0x0063;

fn cpu_with(code: &[u16]) -> Cpu {
    let mut cpu = Cpu::new();
    let words: Vec<_> = code.iter().enumerate().map(|(i, &w)| (i as u16, w)).collect();
    cpu.load_program(&words);
    cpu
}

fn steps(cpu: &mut Cpu, n: usize) {
    for _ in 0..n {
        cpu.step();
    }
}

fn flags(cpu: &Cpu) -> (bool, bool, bool) {
    let s = cpu.status();
    (s >> C & 1 == 1, s >> DC & 1 == 1, s >> Z & 1 == 1)
}

#[test]
fn power_on_values() {
    let cpu = Cpu::new();
    assert_eq!(cpu.status(), 0x18);
    assert_eq!(cpu.peek(OPTION), 0xFF);
    assert_eq!(cpu.peek(TRISA), 0x1F);
    assert_eq!(cpu.peek(TRISB), 0xFF);
    assert_eq!(cpu.pc, 0);
}

#[test]
fn addlw_flags() {
    let mut cpu = cpu_with(&[movlw(0x0F), addlw(0x01), movlw(0xFF), addlw(0x01)]);
    steps(&mut cpu, 2);
    assert_eq!(cpu.w, 0x10);
    assert_eq!(flags(&cpu), (false, true, false));
    steps(&mut cpu, 2);
    assert_eq!(cpu.w, 0x00);
    assert_eq!(flags(&cpu), (true, true, true));
}

#[test]
fn sublw_flags_mean_no_borrow() {
    // k - W
    let cases = [
        (0x02, 0x01, 0x01, true, true, false),
        (0x02, 0x02, 0x00, true, true, true),
        (0x01, 0x02, 0xFF, false, false, false),
        (0x10, 0x01, 0x0F, true, false, false),
    ];
    for (k, w, r, c, dc, z) in cases {
        let mut cpu = cpu_with(&[movlw(w), sublw(k)]);
        steps(&mut cpu, 2);
        assert_eq!(cpu.w, r, "{k:#x} - {w:#x}");
        assert_eq!(flags(&cpu), (c, dc, z), "{k:#x} - {w:#x}");
    }
}

#[test]
fn subwf_into_file() {
    let mut cpu = cpu_with(&[movlw(0x05), movwf(0x20), movlw(0x03), subwf(0x20, true)]);
    steps(&mut cpu, 4);
    assert_eq!(cpu.peek(0x20), 0x02);
    assert_eq!(flags(&cpu), (true, true, false));
}

#[test]
fn rotate_through_carry() {
    let mut cpu = cpu_with(&[movlw(0x81), movwf(0x20), rlf(0x20, true), rrf(0x20, false)]);
    steps(&mut cpu, 3);
    assert_eq!(cpu.peek(0x20), 0x02);
    assert!(cpu.status() & 1 == 1);
    cpu.step();
    assert_eq!(cpu.w, 0x81);
    assert!(cpu.status() & 1 == 0);
}

#[test]
fn swapf_and_movf_zero() {
    let mut cpu = cpu_with(&[movlw(0xA5), movwf(0x20), swapf(0x20, false), clrf(0x21), movf(0x21, true)]);
    steps(&mut cpu, 3);
    assert_eq!(cpu.w, 0x5A);
    steps(&mut cpu, 2);
    assert!(flags(&cpu).2);
}

#[test]
fn bank_switching_and_mirror() {
    let mut cpu = cpu_with(&[bsf(STATUS, RP0), movlw(0x0F), movwf(0x06), movwf(0x20), bcf(STATUS, RP0)]);
    steps(&mut cpu, 5);
    assert_eq!(cpu.peek(TRISB), 0x0F);
    // GPR 0x20 ist in Bank 1 gespiegelt (0xA0)
    assert_eq!(cpu.peek(0x20), 0x0F);
    assert_eq!(cpu.peek(0xA0), 0x0F);
    assert_eq!(cpu.status() & (1 << RP0), 0);
}

#[test]
fn indirect_addressing() {
    let mut cpu = cpu_with(&[movlw(0x30), movwf(FSR), movlw(0x42), movwf(INDF), movf(INDF, false)]);
    steps(&mut cpu, 5);
    assert_eq!(cpu.peek(0x30), 0x42);
    assert_eq!(cpu.w, 0x42);
}

#[test]
fn clrf_status_keeps_z() {
    let mut cpu = cpu_with(&[clrf(STATUS)]);
    cpu.step();
    // Z wird vom Befehl gesetzt, TO/PD bleiben
    assert_eq!(cpu.status(), 0x1C);
}

#[test]
fn call_return_and_cycles() {
    let mut cpu = cpu_with(&[call(3), NOP, goto(1), retlw(0x77)]);
    cpu.step();
    assert_eq!(cpu.pc, 3);
    assert_eq!(cpu.cycles, 2);
    assert_eq!(cpu.sp, 1);
    cpu.step();
    assert_eq!(cpu.pc, 1);
    assert_eq!(cpu.w, 0x77);
    assert_eq!(cpu.cycles, 4);
    assert_eq!(cpu.sp, 0);
}

#[test]
fn decfsz_loop_cycles() {
    // 3 Durchläufe: decfsz 1 Zyklus + goto 2 Zyklen, letzter decfsz 2 Zyklen
    let mut cpu = cpu_with(&[movlw(3), movwf(0x20), decfsz(0x20, true), goto(2), NOP]);
    while cpu.pc != 4 {
        cpu.step();
    }
    assert_eq!(cpu.cycles, 2 + 3 + 3 + 2);
}

#[test]
fn computed_goto_uses_pclath() {
    let mut cpu = cpu_with(&[movlw(0x01), movwf(PCLATH), movlw(0x05), movwf(PCL)]);
    steps(&mut cpu, 3);
    let before = cpu.cycles;
    cpu.step();
    assert_eq!(cpu.pc, 0x105);
    assert_eq!(cpu.cycles - before, 2);
}

#[test]
fn addwf_pcl_table() {
    // addwf PCL springt relativ zum nächsten Befehl
    let mut cpu = cpu_with(&[movlw(2), addwf(PCL, true), retlw(0), retlw(1), retlw(2)]);
    steps(&mut cpu, 2);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn stack_wraps_after_eight() {
    let code: Vec<u16> = (0..9).map(|i| call(i + 1)).collect();
    let mut cpu = cpu_with(&code);
    steps(&mut cpu, 9);
    assert_eq!(cpu.sp, 1);
    // Der neunte CALL hat den ersten Eintrag überschrieben
    assert_eq!(cpu.stack[0], 9);
}

#[test]
fn timer0_internal_clock_with_prescaler() {
    // OPTION = 0x00: T0CS=0, PSA=0, 1:2
    let mut cpu = cpu_with(&[bsf(STATUS, RP0), clrf(0x01), bcf(STATUS, RP0), goto(3)]);
    steps(&mut cpu, 3);
    let t0 = cpu.peek(TMR0);
    steps(&mut cpu, 10); // goto = 2 Zyklen
    assert_eq!(cpu.peek(TMR0).wrapping_sub(t0), 10);
}

#[test]
fn timer0_overflow_sets_t0if() {
    let mut cpu = cpu_with(&[bsf(STATUS, RP0), movlw(0x08), movwf(0x01), bcf(STATUS, RP0), goto(4)]);
    steps(&mut cpu, 4);
    while cpu.peek(INTCON) & (1 << T0IF) == 0 {
        cpu.step();
        assert!(cpu.cycles < 1000);
    }
    assert_eq!(cpu.peek(TMR0), 0);
}

#[test]
fn rb0_interrupt() {
    // GIE + INTE, steigende Flanke (INTEDG=1 nach Reset)
    let mut cpu = cpu_with(&[movlw(0x90), movwf(INTCON), goto(2), NOP, RETFIE]);
    steps(&mut cpu, 3);
    cpu.set_input(1, 0, true);
    assert_eq!(cpu.step(), Event::Interrupt);
    assert_eq!(cpu.pc, 4);
    assert_eq!(cpu.peek(INTCON) & (1 << GIE), 0);
    assert_ne!(cpu.peek(INTCON) & (1 << INTF), 0);
    cpu.step();
    assert_eq!(cpu.pc, 2);
    assert_ne!(cpu.peek(INTCON) & (1 << GIE), 0);
}

#[test]
fn rb_port_change_flag() {
    let mut cpu = cpu_with(&[movf(PORTB, false), goto(1)]);
    cpu.step();
    cpu.set_input(1, 5, true);
    cpu.step();
    assert_ne!(cpu.peek(INTCON) & (1 << RBIF), 0);
}

#[test]
fn ports_follow_tris() {
    let mut cpu = cpu_with(&[bsf(STATUS, RP0), clrf(0x06), bcf(STATUS, RP0), movlw(0xA5), movwf(PORTB)]);
    steps(&mut cpu, 5);
    assert_eq!(cpu.pins()[1], 0xA5);
    // Eingänge an Port A (TRIS=1) kommen von außen
    cpu.set_input(0, 2, true);
    assert_eq!(cpu.peek(PORTA), 0x04);
}

#[test]
fn eeprom_write_then_read() {
    let code = [
        movlw(0x05), movwf(EEADR),
        movlw(0x99), movwf(EEDATA),
        bsf(STATUS, RP0),
        bsf(0x08, 2),             // WREN
        movlw(0x55), movwf(0x09), // EECON2
        movlw(0xAA), movwf(0x09),
        bsf(0x08, 1),             // WR
        btfss(0x08, 4),           // warten auf EEIF
        goto(11),
        bsf(0x08, 0),             // RD
        NOP,
    ];
    let mut cpu = cpu_with(&code);
    while cpu.pc != 14 {
        cpu.step();
        assert!(cpu.cycles < 20_000);
    }
    assert_eq!(cpu.eeprom[5], 0x99);
    assert_eq!(cpu.peek(EECON1) & 0x02, 0);
}

#[test]
fn eeprom_write_needs_sequence() {
    let code = [movlw(0x99), movwf(EEDATA), bsf(STATUS, RP0), bsf(0x08, 2), bsf(0x08, 1), goto(5)];
    let mut cpu = cpu_with(&code);
    steps(&mut cpu, 10_000);
    assert_eq!(cpu.eeprom[0], 0);
}

#[test]
fn watchdog_resets_without_clrwdt() {
    let mut cpu = cpu_with(&[goto(0)]);
    cpu.wdt_enabled = true;
    cpu.poke(OPTION, 0x00);
    let mut ev = Event::None;
    for _ in 0..20_000 {
        ev = cpu.step();
        if ev != Event::None {
            break;
        }
    }
    assert_eq!(ev, Event::WatchdogReset);
    assert_eq!(cpu.status() & (1 << TO), 0);
}

#[test]
fn clrwdt_keeps_watchdog_quiet() {
    let mut cpu = cpu_with(&[CLRWDT, goto(0)]);
    cpu.wdt_enabled = true;
    cpu.poke(OPTION, 0x00);
    for _ in 0..50_000 {
        assert_eq!(cpu.step(), Event::None);
    }
}

#[test]
fn sleep_wakes_on_watchdog() {
    let mut cpu = cpu_with(&[SLEEP, NOP]);
    cpu.wdt_enabled = true;
    cpu.poke(OPTION, 0x00);
    cpu.step();
    assert!(cpu.sleeping);
    let mut ev = Event::None;
    for _ in 0..30_000 {
        ev = cpu.step();
        if ev != Event::None {
            break;
        }
    }
    assert_eq!(ev, Event::WakeUp);
    assert_eq!(cpu.pc, 1);
}

#[test]
fn testo01_runs() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/Testo01.LST");
    let lst = Listing::load(std::path::Path::new(path)).unwrap();
    let mut cpu = Cpu::new();
    cpu.load_program(&lst.program());
    // RB0 = 0: LED an RA0 geht an
    steps(&mut cpu, 6);
    assert!(cpu.pin(0, 0));
    cpu.set_input(1, 0, true);
    steps(&mut cpu, 6);
    assert!(!cpu.pin(0, 0));
}
