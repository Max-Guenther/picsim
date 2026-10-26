//! Fensterlayout: Jeder Bereich ist ein eigenes Fenster, das sich
//! verschieben, größer und kleiner ziehen und ausblenden lässt.
//!
//! Die Lage jedes Fensters wird als Anteil der Fläche gespeichert. Ändert
//! sich die Größe des Programmfensters, werden alle Fenster anteilig
//! mitskaliert, statt am Rand übereinandergeschoben zu werden.
//! Den festen Modus (Kacheln mit Trennern) regelt `tiles`.

use eframe::egui::{self, vec2, Rect, Vec2};
use picsim_core::Ini;
use std::time::{Duration, Instant};

/// Schlüssel, Titel und Standardlage als Anteil der Fläche (x, y, Breite, Höhe).
pub const WINDOWS: [(&str, &str, [f32; 4]); 12] = [
    ("ram", "Datenspeicher", [0.0, 0.0, 0.2, 0.45]),
    ("sfr", "Spezialfunktionsregister", [0.0, 0.45, 0.2, 0.55]),
    ("ports", "Ports", [0.2, 0.0, 0.22, 0.24]),
    ("lcd", "LCD", [0.42, 0.0, 0.2, 0.24]),
    ("sseg", "7-Segmentanzeigen", [0.62, 0.0, 0.25, 0.24]),
    ("switches", "Schalter", [0.2, 0.24, 0.17, 0.27]),
    ("keys", "Taster", [0.37, 0.24, 0.08, 0.27]),
    ("leds", "LED-Array", [0.45, 0.24, 0.11, 0.27]),
    ("stepper", "Schrittmotor", [0.56, 0.24, 0.11, 0.27]),
    ("fgen", "Funktionsgenerator", [0.67, 0.24, 0.2, 0.27]),
    ("listing", "Listing", [0.2, 0.51, 0.67, 0.49]),
    ("control", "Steuerpult", [0.87, 0.0, 0.13, 1.0]),
];

pub const N: usize = WINDOWS.len();
const SEC: &str = "FENSTER";
/// So lange muss die Größe stillstehen, bevor die Fenster angepasst werden.
const SETTLE: Duration = Duration::from_millis(200);
/// Abstand zwischen den Fenstern in Pixeln.
const GAP: f32 = 4.0;
/// Titelleiste und Rand, solange ein Fenster noch nicht gemessen ist.
const DEFAULT_DECO: Vec2 = Vec2::new(14.0, 40.0);

pub struct Layout {
    /// Fenster frei verschiebbar (true) oder fest angeordnet (false).
    /// None: noch nicht gewählt, beim Start wird gefragt.
    pub free: Option<bool>,
    /// Wird erhöht, wenn die Fenster neu aufgebaut werden sollen.
    generation: u32,
    pub visible: [bool; N],
    /// Außenmaße jedes Fensters als Anteil der Fläche (x, y, Breite, Höhe).
    frac: [[f32; 4]; N],
    /// Unterschied zwischen Außen- und Innengröße (Titelleiste, Rand).
    deco: [Vec2; N],
    /// Fläche, zu der `frac` gehört.
    area: Option<Rect>,
    /// Neue Fläche und Zeitpunkt der letzten Änderung, solange noch gezogen wird.
    pending: Option<(Rect, Instant)>,
    /// Bilder seit dem letzten Neuaufbau. Neue egui-Fenster melden in den
    /// ersten Bildern noch vorläufige Größen, die dürfen nicht gespeichert werden.
    frames: u32,
    /// Aufteilung im festen Modus.
    pub fixed: crate::tiles::Split,
    /// Trenner aus dem letzten Bild (Lage auf dem Bildschirm).
    pub grips: Vec<crate::tiles::Handle>,
}

impl Default for Layout {
    fn default() -> Self {
        // Frische Fenster-IDs je Sitzung, damit keine alten Pixellagen greifen
        let session = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        Layout {
            free: None,
            generation: session,
            visible: [true; N],
            frac: WINDOWS.map(|w| w.2),
            deco: [DEFAULT_DECO; N],
            area: None,
            pending: None,
            frames: 0,
            fixed: crate::tiles::default_tree(),
            grips: Vec::new(),
        }
    }
}

fn parse_frac(s: &str) -> Option<[f32; 4]> {
    let v: Vec<f32> = s.split(';').filter_map(|p| p.trim().parse().ok()).collect();
    let f: [f32; 4] = v.try_into().ok()?;
    let ok = f.iter().all(|x| (0.0..=1.0).contains(x)) && f[2] > 0.01 && f[3] > 0.01;
    ok.then_some(f)
}

impl Layout {
    pub fn load(ini: &Ini) -> Self {
        let mut l = Layout::default();
        l.free = ini.get(SEC, "FREI").map(|v| v != "0");
        for (i, (key, _, _)) in WINDOWS.iter().enumerate() {
            l.visible[i] = ini.get(SEC, key).map(|v| v != "0").unwrap_or(true);
            if let Some(f) = ini.get(SEC, &format!("{key}_lage")).and_then(parse_frac) {
                l.frac[i] = f;
            }
        }
        l.fixed.load(ini);
        l
    }

    pub fn store(&self, ini: &mut Ini) {
        if let Some(free) = self.free {
            ini.set_flag(SEC, "FREI", free);
        }
        for (i, (key, _, _)) in WINDOWS.iter().enumerate() {
            ini.set_flag(SEC, key, self.visible[i]);
            let [x, y, w, h] = self.frac[i];
            ini.set(SEC, &format!("{key}_lage"), format!("{x:.4};{y:.4};{w:.4};{h:.4}"));
        }
        self.fixed.store(ini);
    }

    pub fn is_free(&self) -> bool {
        self.free != Some(false)
    }

    /// Modus wechseln. Beim Wechsel zu freien Fenstern starten diese an
    /// ihrer gespeicherten Lage.
    pub fn set_free(&mut self, free: bool) {
        if !self.is_free() && free {
            self.rebuild();
        }
        self.free = Some(free);
    }

    /// Einträge für das Menü "Ansicht". Liefert true bei Änderung.
    pub fn menu(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.label("Fenster");
        for (i, (_, title, _)) in WINDOWS.iter().enumerate() {
            changed |= ui.checkbox(&mut self.visible[i], *title).changed();
        }
        ui.separator();
        if ui.button("Alle Fenster anzeigen").clicked() {
            self.visible = [true; N];
            changed = true;
        }
        if ui.button("Layout zurücksetzen").on_hover_text("Alle Bereiche wieder an ihren Standardplatz und in Standardgröße").clicked() {
            self.visible = [true; N];
            self.frac = WINDOWS.map(|w| w.2);
            self.fixed = crate::tiles::default_tree();
            self.rebuild();
            changed = true;
        }
        changed
    }

    /// Pro Bild vor den Fenstern aufrufen. Hat sich die Fläche geändert und
    /// steht sie kurz still, werden alle Fenster anteilig neu aufgebaut.
    /// Liefert true, solange eine Anpassung aussteht (dann neu zeichnen).
    pub fn update(&mut self, area: Rect) -> bool {
        self.frames = self.frames.saturating_add(1);
        let Some(old) = self.area else {
            self.area = Some(area);
            return false;
        };
        let differs = |a: Rect, b: Rect| (a.min - b.min).length() + (a.size() - b.size()).length() > 1.0;
        match self.pending {
            Some((target, since)) => {
                if differs(target, area) {
                    self.pending = Some((area, Instant::now()));
                } else if since.elapsed() >= SETTLE {
                    self.pending = None;
                    self.area = Some(area);
                    self.rebuild();
                }
                true
            }
            None if differs(old, area) => {
                self.pending = Some((area, Instant::now()));
                true
            }
            None => false,
        }
    }

    fn rebuild(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.frames = 0;
    }

    /// Fläche, nach der die Fenster gerade ausgerichtet sind.
    pub fn area(&self, fallback: Rect) -> Rect {
        self.area.unwrap_or(fallback)
    }

    /// Fenster i, verschiebbar und in der Größe änderbar, innerhalb von `area`.
    pub fn window(&self, i: usize, area: Rect) -> egui::Window<'static> {
        let (key, title, _) = WINDOWS[i];
        let [x, y, w, h] = self.frac[i];
        let outer = Rect::from_min_size(
            area.min + vec2(x * area.width(), y * area.height()),
            vec2(w * area.width(), h * area.height()),
        );
        // Kleiner Abstand zum Nachbarn, Titelleiste und Rand abziehen
        let inner_size = (outer.size() - self.deco[i] - Vec2::splat(GAP)).max(vec2(60.0, 40.0));
        let inner = Rect::from_min_size(outer.min + Vec2::splat(GAP / 2.0), inner_size);
        egui::Window::new(title)
            .id(egui::Id::unique(("fenster", key, self.generation)))
            .default_rect(inner)
            .constrain_to(area)
            .resizable(true)
            .collapsible(true)
            .min_size([60.0, 40.0])
    }

    /// Nach dem Zeichnen: aktuelle Lage eines Fensters merken.
    /// `inner` ist der Inhaltsbereich (None, wenn eingeklappt).
    /// `by_user`: Die Maus ist gerade gedrückt oder wurde losgelassen, das
    /// Fenster kann also verschoben oder in der Größe geändert worden sein.
    pub fn record(&mut self, i: usize, outer: Rect, inner: Option<Rect>, by_user: bool) {
        // Während eine Größenänderung aussteht, sind die Lagen verfälscht
        let (Some(area), None) = (self.area, &self.pending) else { return };
        if self.frames < 3 {
            return;
        }
        if let Some(inner) = inner {
            self.deco[i] = (outer.size() - inner.size()).max(Vec2::ZERO);
        }
        if !by_user {
            return;
        }
        if inner.is_some() {
            let x = (outer.min.x - area.min.x - GAP / 2.0) / area.width();
            let y = (outer.min.y - area.min.y - GAP / 2.0) / area.height();
            let w = (outer.width() + GAP) / area.width();
            let h = (outer.height() + GAP) / area.height();
            self.frac[i] = [x, y, w, h].map(|v| v.clamp(0.0, 1.0));
        } else {
            // Eingeklappt: nur die Position merken, die Größe bleibt
            self.frac[i][0] = ((outer.min.x - area.min.x - GAP / 2.0) / area.width()).clamp(0.0, 1.0);
            self.frac[i][1] = ((outer.min.y - area.min.y - GAP / 2.0) / area.height()).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn area(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 30.0), vec2(w, h))
    }

    #[test]
    fn fractions_survive_ini() {
        let mut l = Layout::default();
        l.frac[3] = [0.1, 0.2, 0.3, 0.4];
        let mut ini = Ini::default();
        l.store(&mut ini);
        let l2 = Layout::load(&ini);
        for (a, b) in l2.frac[3].iter().zip([0.1, 0.2, 0.3, 0.4]) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn record_and_rescale_keep_proportions() {
        let mut l = Layout::default();
        let a1 = area(1600.0, 900.0);
        l.update(a1);
        // Fenster liegt bei 25 % / 50 % und ist halb so breit wie die Fläche
        let outer = Rect::from_min_size(pos2(400.0 + GAP / 2.0, 30.0 + 450.0 + GAP / 2.0), vec2(800.0 - GAP, 300.0 - GAP));
        let inner = Rect::from_min_size(outer.min + vec2(7.0, 30.0), outer.size() - vec2(14.0, 40.0));
        l.frames = 10;
        l.record(0, outer, Some(inner), true);
        let f = l.frac[0];
        assert!((f[0] - 0.25).abs() < 1e-3 && (f[1] - 0.5).abs() < 1e-3 && (f[2] - 0.5).abs() < 1e-3);

        // Programmfenster wird kleiner: erst nach kurzer Ruhe neu aufbauen
        let gen = l.generation;
        let a2 = area(800.0, 450.0);
        assert!(l.update(a2));
        assert_eq!(l.generation, gen);
        l.pending = Some((a2, Instant::now() - SETTLE));
        l.update(a2);
        assert_eq!(l.generation, gen + 1);
        assert_eq!(l.area, Some(a2));
        assert_eq!(l.frac[0], f, "Anteile bleiben beim Verkleinern gleich");
    }

    #[test]
    fn mode_survives_ini() {
        let mut ini = Ini::default();
        assert_eq!(Layout::load(&ini).free, None, "ohne Eintrag wird gefragt");
        let mut l = Layout::default();
        l.set_free(false);
        l.store(&mut ini);
        let l2 = Layout::load(&ini);
        assert_eq!(l2.free, Some(false));
        assert!(!l2.is_free());
    }

    #[test]
    fn bad_ini_values_are_ignored() {
        assert_eq!(parse_frac("0.1;0.2;0.3"), None);
        assert_eq!(parse_frac("0.1;0.2;5;0.3"), None);
        assert_eq!(parse_frac("0.1;0.2;0.3;0.4"), Some([0.1, 0.2, 0.3, 0.4]));
    }
}
