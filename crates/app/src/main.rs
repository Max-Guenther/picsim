//! PicSim: Simulator für den PIC16F84 mit Assembler und Editor.
//!
//! Aufruf wie beim Original: `picsim [datei[.lst]]`. Eine `.src`-Datei
//! wird im Editor geöffnet und gleich übersetzt.

// Unter Windows kein Konsolenfenster neben dem Programm (nur im Release-Build).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assoc;
mod apps;
mod canvas;
mod dialog;
mod editor;
mod layout;
mod periph;
mod project;
mod reference;
mod settings;
mod theme;
mod tiles;
mod update;
mod view;

use eframe::egui;

/// Fenstersymbol: stilisierter Chip mit Beinchen, im Code gezeichnet.
fn icon() -> egui::IconData {
    const N: usize = 64;
    let mut rgba = vec![0u8; N * N * 4];
    let mut put = |x: usize, y: usize, c: [u8; 4]| {
        let i = (y * N + x) * 4;
        rgba[i..i + 4].copy_from_slice(&c);
    };
    for y in 0..N {
        for x in 0..N {
            let body = (14..50).contains(&x) && (8..56).contains(&y);
            let pin = ((6..14).contains(&x) || (50..58).contains(&x)) && (3..=5).contains(&(y % 8)) && (10..54).contains(&y);
            let notch = (x as i32 - 32).pow(2) + (y as i32 - 8).pow(2) < 25;
            if body && !notch {
                put(x, y, [40, 44, 52, 255]);
            } else if pin {
                put(x, y, [200, 200, 210, 255]);
            }
            if body && (24..40).contains(&x) && (26..38).contains(&y) {
                put(x, y, [60, 200, 90, 255]);
            }
        }
    }
    egui::IconData { rgba, width: N as u32, height: N as u32 }
}

fn main() -> eframe::Result {
    let arg = std::env::args().nth(1);
    assoc::register();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("PicSim {}", env!("CARGO_PKG_VERSION")))
            .with_app_id("picsim")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(icon()),
        ..Default::default()
    };
    eframe::run_native(
        "PicSim",
        options,
        Box::new(move |cc| Ok(Box::new(app::PicSimApp::new(cc, arg)))),
    )
}
