# Änderungen

## 0.7.2

- Doppelklick auf eine `.picsim`-Datei öffnet PicSim mit dem Projekt. PicSim trägt sich dafür beim Start selbst als Programm für Projektdateien ein, nur für den eigenen Benutzer und ohne Adminrechte. Unter Windows wird der Pfad bei jedem Start erneuert, falls `picsim.exe` verschoben wurde. Auf dem Mac geht das über die PicSim.app, auch wenn PicSim schon läuft

## 0.7.1

- Fester Modus: Bereiche lassen sich auch an den Ecken greifen. Wo sich zwei Trenner treffen, ändert Ziehen Breite und Höhe auf einmal

## 0.7.0

- Projektdateien: Datei > Projekt speichern legt alles in eine `.picsim`-Datei, die du weitergeben kannst. Darin stecken Quelltext samt INCLUDE-Dateien, alle Einstellungen (Pins, Schalternamen, LED-Farben, Applikationen, Tempo, Tabbreite), das Layout mit allen Größen, die Schalterstellungen und die Haltepunkte. Wer die Datei mit Datei > Projekt öffnen lädt, hat alles genauso und das Programm ist gleich übersetzt und geladen. Eigene Dateipfade, Zoom, Darstellung und Update-Einstellung bleiben dabei beim Empfänger, wie sie sind
- Solange ein Projekt offen ist, aktualisiert Speichern im Editor auch die Projektdatei
- Fester Modus: Die Anordnung bleibt vorgegeben, aber die Trenner zwischen den Bereichen lassen sich mit der Maus ziehen. Die Größen bleiben beim nächsten Start erhalten. Ausgeblendete Bereiche geben ihren Platz an die Nachbarn ab, statt eine Lücke zu lassen. Ansicht > Layout zurücksetzen stellt die Standardgrößen wieder her
- Dark Mode: Einstellungen > Darstellung mit "Wie System" (Standard), "Hell" und "Dunkel". Editor, Listing und Nachschlagefenster haben passende dunkle Farben
- Absturz auf dem Mac behoben: Bewegte man im Fenster "Speichern unter" oder "Öffnen" das Trackpad oder die Maus auf bestimmte Weise, beendete sich PicSim sofort. Alle Dateidialoge laufen jetzt, ohne das Programm anzuhalten, auf dem Mac als Fenster, das am PicSim-Fenster hängt

## 0.6.2

- Werte im Datenspeicher ändern funktioniert wieder: Zelle anklicken, Hexwert eintippen, Enter. Vorher blieb der alte Wert stehen. Escape oder Wegklicken bricht ab
- Beschriftungen in gezeichneten Bauteilen (Schalter, Taster, LCD, 7-Segment, Ampel, Getränkeautomat) bleiben scharf, egal wie groß das Fenster ist. Vorher wurde der Text klein gesetzt und dann hochgezogen und sah dadurch pixelig aus

## 0.6.1

- Kopieren, Ausschneiden und Einfügen im Editor funktionieren jetzt auch mit anderen Programmen. Vorher landete kopierter Text nur in PicSims eigener Zwischenablage. Auf dem Mac geht das wie gewohnt mit Cmd+C, Cmd+X und Cmd+V
- Links im Programm (Info, "Was ist neu?") öffnen wieder den Browser

## 0.6.0

- Tab im Editor springt jetzt zum nächsten Tabstopp statt immer gleich weit. Nach `status` und nach `porta` landet der Befehl also in derselben Spalte
- Tabstopps alle 8 Zeichen statt 4. Die Breite lässt sich oben im Editor unter "Tabbreite" ändern (2 bis 16) und bleibt beim nächsten Start erhalten
- In der Datei bleiben echte Tabzeichen, der Assembler vom Prof liest sie also wie bisher

## 0.5.0

- PicSim gibt es jetzt auch für Windows (64 Bit): `picsim-windows-x86_64.zip` laden, entpacken, `picsim.exe` starten. Der eingebaute Updater funktioniert dort genauso

## 0.4.0

- Fenstermodus wählbar unter Einstellungen > Fenster frei verschieben. An: Jeder Bereich ist ein eigenes Fenster wie in 0.3.0. Aus: Alle Bereiche liegen fest angeordnet im Programmfenster und wachsen mit, nichts schiebt sich übereinander
- Beim ersten Start fragt PicSim, welche Anordnung du möchtest, und zeigt, wo du das später wieder änderst

## 0.3.0

Alles lässt sich selbst anordnen und in der Größe ändern:

- Jeder Bereich ist ein eigenes Fenster: Datenspeicher, Spezialfunktionsregister, Ports, LCD, 7-Segment, Schalter, Taster, LED-Array, Schrittmotor, Funktionsgenerator, Listing und Steuerpult. Fenster an der Titelleiste verschieben, an der Ecke unten rechts größer oder kleiner ziehen, mit dem Pfeil einklappen, mit dem Kreuz ausblenden
- Gezeichnete Bauteile (LCD, 7-Segment, Schalter, Taster, LEDs, Schrittmotor) wachsen und schrumpfen mit dem Fenster, ebenso Ampelkreuzung und Getränkeautomat
- Lage und Größe aller Fenster bleiben beim nächsten Start erhalten
- Menü Ansicht: Fenster ein- und ausblenden, "Alle Fenster anzeigen", "Layout zurücksetzen"
- Schriftgröße weiter über Ansicht oder mit Strg + und Strg -
- Editor, Einstellungen und Applikationen liegen immer vor den Bereichsfenstern
- Wird das Programmfenster größer oder kleiner, wachsen und schrumpfen alle Fenster anteilig mit, statt sich am Rand zu stapeln
- Einstellungen der Bauteile jetzt per Rechtsklick auf das Bauteil (wie im Original), dadurch lassen sich die Fenster schmaler ziehen
- Neues Nachschlagefenster (Hilfe oder Button "Befehle" im Editor): alle Befehle mit Opcode, Zyklen und Flags (Datenblatt Tabelle 9-2), Pseudobefehle und Direktiven, dazu die Registerübersicht (Tabelle 4-1) mit den aktuellen Werten aus der Simulation
- Im Nachschlagefenster ist alles anklickbar: Befehle, Register und Bits werden an der Cursorposition im Editor eingefügt, Bits gleich passend zum Assembler (z.B. STATUS,RP0, PORTB,0 oder GIE)

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
