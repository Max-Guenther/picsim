//! Fester Modus: Die Bereiche liegen in Zeilen und Spalten nebeneinander.
//! Die Anordnung ist vorgegeben, die Größen lassen sich an den Trennern
//! zwischen zwei Nachbarn ziehen. Ausgeblendete Bereiche geben ihren Platz
//! an die Nachbarn ab.

use eframe::egui::{pos2, Rect};
use picsim_core::Ini;

/// Mindestgröße eines Bereichs beim Ziehen in Pixeln.
const MIN_PX: f32 = 60.0;
/// Breite der Fläche, an der sich ein Trenner greifen lässt.
const GRIP: f32 = 8.0;
const SEC: &str = "FENSTER";

pub enum Node {
    /// Bereich i aus `layout::WINDOWS`.
    Leaf(usize),
    Split(Split),
}

pub struct Split {
    /// Name in der ini.
    pub key: &'static str,
    /// true: Kinder nebeneinander, false: untereinander.
    pub row: bool,
    pub children: Vec<Node>,
    /// Anteile der Kinder (werden über die sichtbaren Kinder normiert).
    pub sizes: Vec<f32>,
}

/// Ein Trenner zwischen den Kindern `a` und `b` einer Aufteilung.
pub struct Handle {
    pub key: &'static str,
    pub a: usize,
    pub b: usize,
    pub row: bool,
    /// Greiffläche auf dem Bildschirm.
    pub rect: Rect,
    /// Pixel je Anteil, um Mausbewegungen umzurechnen.
    px_per_unit: f32,
}

fn split(key: &'static str, row: bool, children: Vec<Node>, sizes: &[f32]) -> Split {
    Split { key, row, children, sizes: sizes.to_vec() }
}

use Node::Leaf;

/// Standardanordnung, entspricht den Startlagen in `layout::WINDOWS`.
pub fn default_tree() -> Split {
    let anzeigen = split("anzeigen", true, vec![Leaf(2), Leaf(3), Leaf(4)], &[0.22, 0.2, 0.25]);
    let geraete = split("geraete", true, vec![Leaf(5), Leaf(6), Leaf(7), Leaf(8), Leaf(9)], &[0.17, 0.08, 0.11, 0.11, 0.2]);
    let mitte = split("mitte", false, vec![Node::Split(anzeigen), Node::Split(geraete), Leaf(10)], &[0.24, 0.27, 0.49]);
    let links = split("links", false, vec![Leaf(0), Leaf(1)], &[0.45, 0.55]);
    split("spalten", true, vec![Node::Split(links), Node::Split(mitte), Leaf(11)], &[0.2, 0.67, 0.13])
}

fn any_visible(node: &Node, visible: &[bool]) -> bool {
    match node {
        Leaf(i) => visible[*i],
        Node::Split(s) => s.children.iter().any(|c| any_visible(c, visible)),
    }
}

impl Split {
    /// Teilt `rect` auf: Rechtecke der sichtbaren Bereiche und die Trenner.
    pub fn arrange(&self, rect: Rect, visible: &[bool], tiles: &mut Vec<(usize, Rect)>, handles: &mut Vec<Handle>) {
        let vis: Vec<usize> = (0..self.children.len()).filter(|&k| any_visible(&self.children[k], visible)).collect();
        let total: f32 = vis.iter().map(|&k| self.sizes[k]).sum();
        if vis.is_empty() || total <= 0.0 {
            return;
        }
        let len = if self.row { rect.width() } else { rect.height() };
        let px_per_unit = len / total;
        let mut pos = if self.row { rect.left() } else { rect.top() };
        for (n, &k) in vis.iter().enumerate() {
            let end = pos + self.sizes[k] * px_per_unit;
            let child = if self.row {
                Rect::from_min_max(pos2(pos, rect.top()), pos2(end, rect.bottom()))
            } else {
                Rect::from_min_max(pos2(rect.left(), pos), pos2(rect.right(), end))
            };
            match &self.children[k] {
                Leaf(i) => tiles.push((*i, child)),
                Node::Split(s) => s.arrange(child, visible, tiles, handles),
            }
            if let Some(&b) = vis.get(n + 1) {
                let grip = if self.row {
                    Rect::from_min_max(pos2(end - GRIP / 2.0, rect.top()), pos2(end + GRIP / 2.0, rect.bottom()))
                } else {
                    Rect::from_min_max(pos2(rect.left(), end - GRIP / 2.0), pos2(rect.right(), end + GRIP / 2.0))
                };
                handles.push(Handle { key: self.key, a: k, b, row: self.row, rect: grip, px_per_unit });
            }
            pos = end;
        }
    }

    fn find_mut(&mut self, key: &str) -> Option<&mut Split> {
        if self.key == key {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| match c {
            Node::Split(s) => s.find_mut(key),
            Leaf(_) => None,
        })
    }

    fn for_each(&self, f: &mut dyn FnMut(&Split)) {
        f(self);
        for c in &self.children {
            if let Node::Split(s) = c {
                s.for_each(f);
            }
        }
    }

    /// Trenner `h` um `delta` Pixel verschieben.
    pub fn drag(&mut self, h: &Handle, delta: f32) {
        let Some(s) = self.find_mut(h.key) else { return };
        let sum = s.sizes[h.a] + s.sizes[h.b];
        let min = (MIN_PX / h.px_per_unit).min(sum / 2.0);
        let a = (s.sizes[h.a] + delta / h.px_per_unit).clamp(min, sum - min);
        s.sizes[h.a] = a;
        s.sizes[h.b] = sum - a;
    }

    pub fn load(&mut self, ini: &Ini) {
        let mut keys = Vec::new();
        self.for_each(&mut |s| keys.push(s.key));
        for key in keys {
            let Some(text) = ini.get(SEC, &format!("fest_{key}")) else { continue };
            let sizes: Vec<f32> = text.split(';').filter_map(|p| p.trim().parse().ok()).collect();
            let Some(s) = self.find_mut(key) else { continue };
            if sizes.len() == s.sizes.len() && sizes.iter().all(|&x| x.is_finite() && x > 0.0) {
                s.sizes = sizes;
            }
        }
    }

    pub fn store(&self, ini: &mut Ini) {
        self.for_each(&mut |s| {
            let text: Vec<String> = s.sizes.iter().map(|x| format!("{x:.4}")).collect();
            ini.set(SEC, &format!("fest_{}", s.key), text.join(";"));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    fn area() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0))
    }

    fn arrange(t: &Split, visible: &[bool]) -> (Vec<(usize, Rect)>, Vec<Handle>) {
        let (mut tiles, mut handles) = (Vec::new(), Vec::new());
        t.arrange(area(), visible, &mut tiles, &mut handles);
        (tiles, handles)
    }

    fn tile(tiles: &[(usize, Rect)], i: usize) -> Rect {
        tiles.iter().find(|(k, _)| *k == i).unwrap().1
    }

    #[test]
    fn default_matches_window_table() {
        let (tiles, handles) = arrange(&default_tree(), &[true; 12]);
        assert_eq!(tiles.len(), 12);
        for &(i, r) in &tiles {
            let [x, y, w, h] = crate::layout::WINDOWS[i].2;
            let want = Rect::from_min_size(pos2(x * 1000.0, y * 800.0), vec2(w * 1000.0, h * 800.0));
            assert!((r.min - want.min).length() < 1.0 && (r.size() - want.size()).length() < 1.0, "Bereich {i}: {r:?} statt {want:?}");
        }
        // 2 Spaltentrenner, 1 links, 2 in der Mitte, 2 + 4 in den Zeilen
        assert_eq!(handles.len(), 11);
    }

    #[test]
    fn tiles_never_overlap_and_fill_area() {
        let (tiles, _) = arrange(&default_tree(), &[true; 12]);
        let sum: f32 = tiles.iter().map(|(_, r)| r.area()).sum();
        assert!((sum - area().area()).abs() < 1.0);
        for (i, a) in &tiles {
            for (j, b) in &tiles {
                if i != j {
                    assert!(a.intersect(*b).area() < 0.01, "{i} und {j} überlappen");
                }
            }
        }
    }

    #[test]
    fn dragging_moves_only_neighbours() {
        let mut t = default_tree();
        let (before, handles) = arrange(&t, &[true; 12]);
        // erster Spaltentrenner: Datenspeicher/Register gegen Mitte
        let h = handles.iter().find(|h| h.key == "spalten" && h.a == 0).unwrap();
        t.drag(h, 100.0);
        let (after, _) = arrange(&t, &[true; 12]);
        assert!((tile(&after, 0).width() - tile(&before, 0).width() - 100.0).abs() < 0.5);
        assert!((tile(&after, 11).width() - tile(&before, 11).width()).abs() < 0.5, "Steuerpult bleibt gleich");
    }

    #[test]
    fn drag_respects_minimum() {
        let mut t = default_tree();
        let (_, handles) = arrange(&t, &[true; 12]);
        let h = handles.iter().find(|h| h.key == "links").unwrap();
        t.drag(h, -10_000.0);
        let (tiles, _) = arrange(&t, &[true; 12]);
        assert!((tile(&tiles, 0).height() - MIN_PX).abs() < 0.5);
    }

    #[test]
    fn hidden_tiles_give_space_to_neighbours() {
        let mut visible = [true; 12];
        visible[1] = false; // Register aus: Datenspeicher füllt die linke Spalte
        let (tiles, handles) = arrange(&default_tree(), &visible);
        assert!((tile(&tiles, 0).height() - 800.0).abs() < 0.5);
        assert!(!handles.iter().any(|h| h.key == "links"));
    }

    #[test]
    fn sizes_survive_ini() {
        let mut t = default_tree();
        let (_, handles) = arrange(&t, &[true; 12]);
        t.drag(&handles[0], 50.0);
        let mut ini = Ini::default();
        t.store(&mut ini);
        let mut t2 = default_tree();
        t2.load(&ini);
        assert!((t2.sizes[0] - t.sizes[0]).abs() < 1e-3);

        ini.set(SEC, "fest_spalten", "1;2");
        let mut t3 = default_tree();
        t3.load(&ini);
        assert_eq!(t3.sizes, default_tree().sizes, "falsche Anzahl wird ignoriert");
    }
}
