//! Bereiche in eigene Fenster des Betriebssystems herauslösen, zum Beispiel
//! den Editor auf einen zweiten Bildschirm oder im Splitscreen neben den
//! Simulator. Schließt man so ein Fenster, kommt der Bereich zurück ins
//! Hauptfenster.

use eframe::egui::{self, Rect, Vec2};
use std::sync::{Arc, OnceLock};

/// INI-Abschnitt: welche Bereiche in eigenen Fenstern stehen. Gehört zum
/// Bildschirm dieses Rechners und wandert darum nicht ins Projekt.
pub const SEC: &str = "EIGENE_FENSTER";

pub const HINT: &str = "In eigenem Fenster öffnen. Schließt du es, kommt der Bereich zurück ins Hauptfenster.";

fn icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| Arc::new(crate::icon())).clone()
}

/// `content` in einem eigenen Fenster zeigen. Jedes Bild aufrufen, solange
/// es offen sein soll. Liefert false, sobald es geschlossen wurde.
pub fn show(ctx: &egui::Context, key: &str, title: &str, size: Vec2, content: impl FnOnce(&mut egui::Ui)) -> bool {
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_app_id("picsim")
        .with_icon(icon())
        .with_inner_size(size.max(Vec2::new(240.0, 160.0)))
        .with_min_inner_size([160.0, 100.0]);
    let mut content = Some(content);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of(("eigenes_fenster", key)), builder, |ui, class| {
        egui::CentralPanel::default_margins().show(ui, |ui| {
            if let Some(c) = content.take() {
                c(ui);
            }
        });
        // Eingebettet (ohne eigene Fenster) gälte das Schließen dem Hauptfenster
        class == egui::ViewportClass::EmbeddedWindow || !ui.input(|i| i.viewport().close_requested())
    })
}

/// Lage des Buttons in der Titelleiste, links neben dem X (falls vorhanden).
/// `outer` ist das ganze Fenster, `content_top` der obere Rand des Inhalts.
fn title_rect(outer: Rect, content_top: f32, size: f32, gap: f32, margin: f32, closable: bool) -> Rect {
    let mut right = outer.right() - margin;
    if closable {
        right -= size + gap;
    }
    let center_y = (outer.top() + content_top) / 2.0;
    Rect::from_center_size(egui::pos2(right - size / 2.0, center_y), Vec2::splat(size))
}

/// Button zum Herauslösen in der Titelleiste eines egui-Fensters. Aus dem
/// Inhalt des Fensters aufrufen, `window` ist dessen Id. Liefert true bei Klick.
pub fn title_button(ui: &mut egui::Ui, window: egui::Id, closable: bool) -> bool {
    // Lage aus dem letzten Bild, im allerersten gibt es noch keine
    let Some(outer) = ui.ctx().memory(|m| m.area_rect(window)) else { return false };
    let size = ui.text_style_height(&egui::TextStyle::Heading);
    let margin = ui.style().spacing.window_margin;
    let content_top = ui.max_rect().top() - margin.topf();
    let rect = title_rect(outer, content_top, size, ui.spacing().item_spacing.x, margin.rightf(), closable);
    ui.ctx().data_mut(|d| d.insert_temp(window.with("herausloesen"), rect));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    // Der Inhalt ist auf seinen Bereich beschnitten, die Titelleiste liegt darüber
    child.set_clip_rect(outer);
    icon_button(&mut child, size).clicked()
}

/// Wo der Button in der Titelleiste von `window` zuletzt lag (für Tests).
#[cfg(test)]
pub fn title_button_rect(ctx: &egui::Context, window: egui::Id) -> Option<Rect> {
    ctx.data(|d| d.get_temp(window.with("herausloesen")))
}

/// Kleiner Button zum Herauslösen, z.B. neben einer Überschrift.
pub fn button(ui: &mut egui::Ui) -> bool {
    let size = ui.text_style_height(&egui::TextStyle::Body);
    icon_button(ui, size).clicked()
}

/// Die Schriften haben kein passendes Zeichen, darum selbst gezeichnet:
/// ein kleines Fenster, aus dem ein Pfeil nach rechts oben zeigt.
fn icon_button(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let resp = resp.on_hover_text(HINT);
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact(&resp);
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 3.0, v.weak_bg_fill);
        }
        let r = rect.shrink(size * 0.2);
        let stroke = egui::Stroke::new(1.3, v.fg_stroke.color);
        let corner = r.right_top();
        let mid = r.lerp_inside(Vec2::new(0.55, 0.45));
        let win = Rect::from_min_max(egui::pos2(r.left(), r.top() + r.height() * 0.3), egui::pos2(r.right() - r.width() * 0.3, r.bottom()));
        p.rect_stroke(win, 1.0, stroke, egui::StrokeKind::Inside);
        p.line_segment([mid, corner], stroke);
        let head = r.width() * 0.35;
        p.line_segment([corner, corner + Vec2::new(-head, 0.0)], stroke);
        p.line_segment([corner, corner + Vec2::new(0.0, head)], stroke);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn title_button_sits_left_of_close() {
        let outer = Rect::from_min_size(pos2(100.0, 50.0), Vec2::new(300.0, 200.0));
        let with_close = title_rect(outer, 80.0, 20.0, 8.0, 6.0, true);
        assert_eq!(with_close.right(), 400.0 - 6.0 - 20.0 - 8.0);
        assert_eq!(with_close.center().y, 65.0);
        let alone = title_rect(outer, 80.0, 20.0, 8.0, 6.0, false);
        assert_eq!(alone.right(), 394.0);
    }
}
