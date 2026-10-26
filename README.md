# PicSim

Simulator für den PIC16F84 mit eingebautem Assembler und Code-Editor, für Linux, macOS (Apple Silicon) und Windows.

PicSim ist eine komplette Neuentwicklung nach dem Vorbild von **PicSimu 0.51** (Stefan Lehmann). Bedienung, Tastenkürzel, LST-Format und `picsimu.ini` sind gleich geblieben. Der Assembler ist kompatibel zu **iL_ASS16**, Wine wird nicht mehr gebraucht.

## Installation

```sh
curl -fsSL https://raw.githubusercontent.com/Max-Guenther/picsim/main/install.sh | sh
```

- **Linux (x86_64):** Programm nach `~/.local/bin/picsim`, Starter im Anwendungsmenü
- **macOS (Apple Silicon):** `PicSim.app` nach `/Applications`

**Windows (64 Bit):** `picsim-windows-x86_64.zip` vom [neuesten Release](https://github.com/Max-Guenther/picsim/releases/latest) laden, entpacken und `picsim.exe` starten. Eine Installation braucht es nicht. Weil das Programm nicht signiert ist, warnt Windows beim ersten Start: auf "Weitere Informationen" und dann "Trotzdem ausführen" klicken.

Danach hält sich PicSim selbst aktuell: Beim Start fragt es GitHub nach einer neuen Version und bietet ein Update an (abschaltbar unter Hilfe).

Selbst bauen geht auch:

```sh
cargo install --git https://github.com/Max-Guenther/picsim picsim
```

Unter Ubuntu braucht das die Pakete `libgtk-3-dev libxkbcommon-dev libwayland-dev`.

## Bedienung

```sh
picsim                 # zuletzt geöffnete Datei
picsim programm.src    # Quelltext im Editor öffnen, assemblieren und laden
picsim programm        # wie im Original: .lst wird ergänzt
```

1. **Quelltext bearbeiten** öffnet den Editor.
2. **Assemblieren & Laden** (F5 oder Strg+B) übersetzt und schreibt `.LST`, `.HEX`, `.ERR` und `.SYM` neben die Quelldatei. Fehler sind im Editor rot markiert, ein Klick auf die Fehlermeldung springt zur Zeile. Tab springt zum nächsten Tabstopp (alle 8 Zeichen, oben unter "Tabbreite" einstellbar).
3. Das **Steuerpult** führt das Programm aus:

| Taste | Funktion |
|---|---|
| F2 | Reset (Power-On-Reset), bricht auch Go ab |
| F6 | Step out: bis zum Rücksprung aus dem Unterprogramm |
| F7 | Ignore: nächsten Befehl überspringen |
| F8 | Step in: einen Befehl ausführen |
| F9 | Step over: Unterprogramm in einem Rutsch |
| F10 | Go: bis zum Breakpoint, nochmal drücken hält an |

Breakpoints setzt ein Klick in die erste Spalte des Listings. RAM-Werte, Bits in Status, Option und INTCON und die Port-Pins lassen sich anklicken und ändern.

## Was simuliert wird

- PIC16F84: alle 35 Befehle mit Flags und Zyklen, Bänke, indirekte Adressierung, Stack (8 Ebenen), Timer0 mit Vorteiler, Watchdog, SLEEP, Interrupts, EEPROM
- LED-Array (9 LEDs, Würfel oder Leuchtband), 16 Schiebeschalter, 3 Taster
- 8 Siebensegmentanzeigen mit Multiplex, gemeinsamer Anode/Kathode und Latch
- LCD (HD44780 im 4-Bit-Modus, 1x8 bis 4x20)
- Funktionsgenerator (Rechteck), Schrittmotor
- Applikationen: Ampeln an einer Kreuzung, Getränkeautomat

Alle Bauteile lassen sich unter **Einstellungen** frei auf Pins legen. Die Einstellungen stehen in `picsimu.ini` (Linux: `~/.config/picsim/`, macOS: `~/Library/Application Support/picsim/`). Die INI des alten Programms lässt sich über **Datei → Einstellungen importieren** übernehmen.

## Assembler

Kompatibel zu iL_ASS16, siehe `examples/`:

- Labels in Spalte 1, Kommentare mit `;`
- `EQU`, `SET`, `ORG`, `END`, `DEVICE`, `CONFIG`, `INCLUDE`, `IF`/`ELSEIF`/`ELSE`/`ENDIF`, `DT`, `RETW`, `DW`
- Zahlen: `35`, `0FFH`, `0x1F`, `$1F`, `01B`, `'A'`, Ausdrücke mit `+ - * / & | ^ << >>`
- Bit-Symbole: `TASTE EQU 6,4` und dann `btfsc TASTE`
- Pseudobefehle von Microchip: `SKPZ`, `SKPC`, `CLRC`, `SETC`, `MOVFW`, `BZ`, `BNZ`, `LGOTO` …

Anders als beim Original sind die Register des 16F84 schon vordefiniert (`STATUS`, `PORTB`, `TRISB`, `GIE` …). Eigene EQUs haben Vorrang, alte Quelltexte übersetzen also unverändert. Für zwei Beispielprogramme ist geprüft, dass Opcodes und Listing mit dem Original übereinstimmen.

## Entwicklung

```sh
cargo test --workspace     # Kern, Assembler, Editor
cargo run -p picsim        # Programm starten
scripts/release.sh 0.2.0   # neue Version veröffentlichen
```

| Ordner | Inhalt |
|---|---|
| `crates/core` | Simulator-Kern: CPU, Peripherie, LST, INI |
| `crates/asm` | Assembler |
| `crates/app` | Oberfläche (egui), Editor, Updater |
