//! Darstellung: wie das System (Standard), immer hell oder immer dunkel.

use eframe::egui::{self, Color32, ThemePreference};
use picsim_core::Ini;

// Gehört zu diesem Rechner, nicht ins Projekt (DISPLAY ist rechnereigen)
const SEC: &str = "DISPLAY";
const KEY: &str = "THEMA";

pub fn from_ini(ini: &Ini) -> ThemePreference {
    match ini.get(SEC, KEY) {
        Some("Hell") => ThemePreference::Light,
        Some("Dunkel") => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

pub fn store(ini: &mut Ini, pref: ThemePreference) {
    let v = match pref {
        ThemePreference::Light => "Hell",
        ThemePreference::Dark => "Dunkel",
        ThemePreference::System => "System",
    };
    ini.set(SEC, KEY, v);
}

/// Auswahl im Menü Einstellungen. Liefert true bei Änderung.
pub fn menu(ui: &mut egui::Ui, pref: &mut ThemePreference) -> bool {
    let mut changed = false;
    ui.label("Darstellung");
    for (p, name) in [(ThemePreference::System, "Wie System"), (ThemePreference::Light, "Hell"), (ThemePreference::Dark, "Dunkel")] {
        changed |= ui.radio_value(pref, p, name).changed();
    }
    changed
}

/// Helle oder dunkle Variante einer Farbe, je nach aktueller Darstellung.
pub fn pick(ui: &egui::Ui, light: Color32, dark: Color32) -> Color32 {
    if ui.visuals().dark_mode {
        dark
    } else {
        light
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_survives_ini() {
        let mut ini = Ini::default();
        assert_eq!(from_ini(&ini), ThemePreference::System, "Standard: wie das System");
        for p in [ThemePreference::Light, ThemePreference::Dark, ThemePreference::System] {
            store(&mut ini, p);
            assert_eq!(from_ini(&ini), p);
        }
    }
}
