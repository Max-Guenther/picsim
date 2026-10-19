# Änderungen

## 0.2.1

- Fehler behoben: Bei Go blieb die grüne Zeile im Listing scheinbar stehen, weil jedes Bild nach gleich vielen Befehlen endete. Jetzt wandert sie mit dem Programm, und das Listing scrollt mit
- Neues Tempo für Go: Maximal, Echtzeit oder Langsam mit einstellbaren Befehlen pro Sekunde (Steuerpult und Menü Ansicht)

## 0.2.0

Applikationen wie im Original (Menü Applikationen):

- Ampeln an einer Kreuzung: 4 Ampeln an frei wählbaren Pins, 4 Autos einzeln freigeben und mit eigenem Tempo, Kontaktschleifen als Eingänge, Crash-Erkennung (Crash anklicken zum Auflösen)
- Getränkeautomat: Münzen (0,50 €, 1 €, 2 €) per Drag & Drop, Münzprüfer-Impuls, Zwischenkasse mit Klappen zur Kasse und Rückgabe, 4 Schächte mit Motor, Leersensor und Wahltaste (Wischerfunktion), Geldrückgabe, Klappe, Kreditanzeige, Auffüllen mit Shift+Klick
- Die Applikationen laufen in Simulationszeit, halten also an, wenn die Simulation steht
- Offene Applikationsfenster und die Autos der Ampel bleiben beim nächsten Start erhalten
- Neue Beispiele: examples/ampel.src und examples/lcd_hallo.src
- Assembler: Text in DT und RETW, z.B. dt "Hallo",0

## 0.1.0

Erste Version, komplett neu geschrieben nach dem Vorbild von PicSimu 0.51.

- Simulator für den PIC16F84: alle 35 Befehle mit Flags und Zyklen, Bänke, indirekte Adressierung, Stack mit 8 Ebenen, Timer0 mit Vorteiler, Watchdog, SLEEP, Interrupts (Timer0, RB0, RB4..7, EEPROM), EEPROM
- Eingebauter Assembler, kompatibel zu iL_ASS16: erzeugt LST, HEX, ERR und SYM. Kein Wine mehr nötig
- Code-Editor mit Syntax-Hervorhebung, Zeilennummern, Fehleranzeige und Assemblieren & Laden (F5)
- Steuerpult wie im Original: Reset (F2), Step out (F6), Ignore (F7), Step in (F8), Step over (F9), Go (F10), Breakpoints im Listing
- Peripherie: LED-Array, 16 Schiebeschalter, Taster, 8 Siebensegmentanzeigen mit Multiplex und Latch, LCD (HD44780, 4 Bit), Funktionsgenerator, Schrittmotor
- Liest und schreibt das picsimu.ini-Format, vorhandene Einstellungen lassen sich importieren
- Updater über GitHub-Releases
