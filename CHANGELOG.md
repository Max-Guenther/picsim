# Änderungen

## 0.1.0

Erste Version, komplett neu geschrieben nach dem Vorbild von PicSimu 0.51.

- Simulator für den PIC16F84: alle 35 Befehle mit Flags und Zyklen, Bänke, indirekte Adressierung, Stack mit 8 Ebenen, Timer0 mit Vorteiler, Watchdog, SLEEP, Interrupts (Timer0, RB0, RB4..7, EEPROM), EEPROM
- Eingebauter Assembler, kompatibel zu iL_ASS16: erzeugt LST, HEX, ERR und SYM. Kein Wine mehr nötig
- Code-Editor mit Syntax-Hervorhebung, Zeilennummern, Fehleranzeige und Assemblieren & Laden (F5)
- Steuerpult wie im Original: Reset (F2), Step out (F6), Ignore (F7), Step in (F8), Step over (F9), Go (F10), Breakpoints im Listing
- Peripherie: LED-Array, 16 Schiebeschalter, Taster, 8 Siebensegmentanzeigen mit Multiplex und Latch, LCD (HD44780, 4 Bit), Funktionsgenerator, Schrittmotor
- Liest und schreibt das picsimu.ini-Format, vorhandene Einstellungen lassen sich importieren
- Updater über GitHub-Releases
