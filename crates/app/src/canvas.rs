//! Zeichenfläche, die mit dem Fenster wächst und schrumpft.
//!
//! Gezeichnet wird in fester Normalgröße. Am Ende werden alle Formen auf
//! die verfügbare Fläche skaliert. Mauspositionen rechnet `to_natural`
//! zurück, damit Klicks in Normalkoordinaten geprüft werden können.

use eframe::egui::{
    self, emath::TSTransform, layers::ShapeIdx, Pos2, Rect, Response, Sense, Shape, Vec2,
};

pub struct Canvas {
    pub painter: egui::Painter,
    /// Zeichenfläche in Normalkoordinaten.
    pub rect: Rect,
    pub k: f32,
    pub resp: Response,
    start: ShapeIdx,
}

impl Canvas {
    /// `reserve` ist die Höhe, die unterhalb für Buttons frei bleiben soll.
    pub fn new(ui: &mut egui::Ui, natural: Vec2, sense: Sense, reserve: f32) -> Self {
        let avail = ui.available_size() - Vec2::new(0.0, reserve);
        let kx = avail.x / natural.x;
        let ky = if avail.y.is_finite() && avail.y > 0.0 {
            avail.y / natural.y
        } else {
            kx
        };
        let k = kx.min(ky).clamp(0.3, 8.0);
        let (screen, resp) = ui.allocate_exact_size(natural * k, sense);
        let rect = Rect::from_min_size(screen.min, natural);
        let painter = egui::Painter::new(ui.ctx().clone(), ui.layer_id(), rect);
        let start = painter.add(Shape::Noop);
        Canvas {
            painter,
            rect,
            k,
            resp,
            start,
        }
    }

    /// Bildschirmposition in Normalkoordinaten.
    pub fn to_natural(&self, p: Pos2) -> Pos2 {
        self.rect.min + (p - self.rect.min) / self.k
    }

    /// Position des Klicks in Normalkoordinaten.
    pub fn click_pos(&self) -> Option<Pos2> {
        if self.resp.clicked() {
            self.resp.interact_pointer_pos().map(|p| self.to_natural(p))
        } else {
            None
        }
    }

    /// Skaliert alles Gezeichnete auf die Bildschirmgröße.
    pub fn finish(self) -> Response {
        let end = self.painter.add(Shape::Noop);
        let origin = self.rect.min.to_vec2();
        let t = TSTransform::new(origin * (1.0 - self.k), self.k);
        let layer = self.painter.layer_id();
        self.painter
            .ctx()
            .graphics_mut(|g| g.entry(layer).transform_range(self.start, end, t));
        self.resp
    }
}
