//! Zeichenfläche, die mit dem Fenster wächst und schrumpft.
//!
//! Gezeichnet wird in fester Normalgröße. Am Ende werden alle Formen auf
//! die verfügbare Fläche skaliert. Mauspositionen rechnet `to_natural`
//! zurück, damit Klicks in Normalkoordinaten geprüft werden können.
//! Text wird dabei in Bildschirmgröße neu gesetzt, sonst wäre er unscharf.

use eframe::egui::{
    self, emath::TSTransform, layers::ShapeIdx, text::LayoutJob, Pos2, Rect, Response, Sense,
    Shape, Vec2,
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
        let ctx = self.painter.ctx();

        // Formen skalieren verlustfrei. Text ist schon in Normalgröße gesetzt,
        // hochgezogen würde er pixelig: deshalb in Bildschirmgröße neu setzen.
        let mut jobs = Vec::new();
        ctx.graphics_mut(|g| {
            let list = g.entry(layer);
            list.transform_range(self.start, end, t);
            for i in self.start.0..end.0 {
                list.mutate_shape(ShapeIdx(i), |cs| {
                    if let Shape::Text(ts) = &cs.shape {
                        jobs.push((i, scaled_job(&ts.galley.job, self.k)));
                    }
                });
            }
        });
        let galleys: Vec<_> = jobs
            .into_iter()
            .map(|(i, job)| (i, ctx.fonts_mut(|f| f.layout_job(job))))
            .collect();
        ctx.graphics_mut(|g| {
            let list = g.entry(layer);
            for (i, galley) in galleys {
                list.mutate_shape(ShapeIdx(i), |cs| {
                    if let Shape::Text(ts) = &mut cs.shape {
                        ts.galley = galley;
                    }
                });
            }
        });
        self.resp
    }
}

/// Derselbe Text, `k`-mal so groß gesetzt.
fn scaled_job(job: &LayoutJob, k: f32) -> LayoutJob {
    let mut job = job.clone();
    for s in &mut job.sections {
        s.leading_space *= k;
        s.format.font_id.size *= k;
        s.format.extra_letter_spacing *= k;
        s.format.line_height = s.format.line_height.map(|h| h * k);
    }
    job.wrap.max_width *= k;
    job.first_row_min_height *= k;
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_scales_fonts() {
        let mut job = LayoutJob::simple_singleline("RA4".into(), egui::FontId::monospace(12.0), egui::Color32::BLACK);
        job.sections[0].leading_space = 1.0;
        let big = scaled_job(&job, 3.0);
        assert_eq!(big.text, "RA4");
        assert_eq!(big.sections[0].format.font_id.size, 36.0);
        assert_eq!(big.sections[0].leading_space, 3.0);
    }

    #[test]
    fn text_is_laid_out_at_screen_size() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 800.0)));
        let mut sizes = Vec::new();
        let mut out = ctx.run_ui(input, |ui| {
            let c = Canvas::new(ui, Vec2::new(100.0, 100.0), Sense::hover(), 0.0);
            c.painter.text(c.rect.center(), egui::Align2::CENTER_CENTER, "S1", egui::FontId::proportional(10.0), egui::Color32::BLACK);
            let k = c.k;
            let layer = c.painter.layer_id();
            c.finish();
            ui.ctx().graphics_mut(|g| {
                let list = g.entry(layer);
                for i in 0..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(i), |cs| {
                        if let Shape::Text(ts) = &cs.shape {
                            sizes.push((k, ts.galley.job.sections[0].format.font_id.size));
                        }
                    });
                }
            });
        });
        out.textures_delta.clear();
        let [(k, size)] = sizes[..] else { panic!("ein Text erwartet: {sizes:?}") };
        assert!(k > 4.0, "Fläche sollte deutlich vergrößert sein, k = {k}");
        assert_eq!(size, 10.0 * k);
    }
}
