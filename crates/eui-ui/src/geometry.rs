//! Map geometry derived from the network: the extent, each link's end nodes
//! and each link's bounding box. Large networks (hundreds of thousands of
//! links) cannot afford to rebuild these every frame, so they are cached and
//! rebuilt only when the network changes.

use eui_core::Network;

#[derive(Default)]
pub struct Geometry {
    /// (document, generation, nodes, links) the topology was built for.
    key: Option<(u64, u64, usize, usize)>,
    /// Node positions or vertices moved without a new generation (dragging).
    moved: bool,
    /// `[min_x, min_y, max_x, max_y]` of all node coordinates and vertices.
    pub bounds: Option<[f64; 4]>,
    /// End node indices of each link.
    pub ends: Vec<Option<(usize, usize)>>,
    /// World bounding box of each link with both ends placed.
    pub boxes: Vec<Option<[f64; 4]>>,
}

impl Geometry {
    /// Rebuilds what changed since the last call.
    pub fn refresh(&mut self, net: &Network, document: u64, generation: u64) {
        let key = (document, generation, net.nodes.len(), net.links.len());
        if self.key != Some(key) {
            self.key = Some(key);
            self.ends = net.links.iter().map(|l| net.link_ends(l)).collect();
            self.moved = true;
        }
        if self.moved {
            self.moved = false;
            self.measure(net);
        }
    }

    /// Positions changed in place (a node or vertex is being dragged).
    pub fn mark_moved(&mut self) {
        self.moved = true;
    }

    fn measure(&mut self, net: &Network) {
        let mut all: Option<[f64; 4]> = None;
        for p in net.nodes.iter().filter_map(|n| n.pos) {
            all = Some(grow(all, p));
        }
        self.boxes = net
            .links
            .iter()
            .zip(&self.ends)
            .map(|(l, e)| {
                let (a, b) = (*e)?;
                let mut bb = grow(Some(grow(None, net.nodes[a].pos?)), net.nodes[b].pos?);
                for &v in &l.vertices {
                    bb = grow(Some(bb), v);
                }
                Some(bb)
            })
            .collect();
        // Vertices of links with an unplaced end still count toward the extent.
        for v in net.links.iter().flat_map(|l| l.vertices.iter().copied()) {
            all = Some(grow(all, v));
        }
        self.bounds = all;
    }
}

fn grow(b: Option<[f64; 4]>, [x, y]: [f64; 2]) -> [f64; 4] {
    match b {
        None => [x, y, x, y],
        Some([a, c, d, e]) => [a.min(x), c.min(y), d.max(x), e.max(y)],
    }
}

/// One bit per screen cell: lets the map skip objects that would land on a
/// spot something has already been drawn on.
pub struct Occupancy {
    cell: f32,
    origin: egui::Pos2,
    cols: usize,
    rows: usize,
    bits: Vec<u64>,
}

impl Occupancy {
    pub fn new(rect: egui::Rect, cell: f32) -> Self {
        let cell = cell.max(1.0);
        let cols = (rect.width() / cell).ceil().max(1.0) as usize + 1;
        let rows = (rect.height() / cell).ceil().max(1.0) as usize + 1;
        Occupancy {
            cell,
            origin: rect.min,
            cols,
            rows,
            bits: vec![0; (cols * rows).div_ceil(64)],
        }
    }

    /// Marks the cell under `p`; false if it was already taken (or off-grid).
    pub fn claim(&mut self, p: egui::Pos2) -> bool {
        let c = ((p.x - self.origin.x) / self.cell).floor();
        let r = ((p.y - self.origin.y) / self.cell).floor();
        if c < 0.0 || r < 0.0 || c as usize >= self.cols || r as usize >= self.rows {
            return true;
        }
        let i = r as usize * self.cols + c as usize;
        let (w, m) = (i / 64, 1u64 << (i % 64));
        let free = self.bits[w] & m == 0;
        self.bits[w] |= m;
        free
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_until_the_network_changes() {
        let mut net = Network::from_inp(
            "[JUNCTIONS]\nA 0\nB 0\n[PIPES]\nP A B 1 1 1\n[COORDINATES]\nA 0 0\nB 10 5\n[VERTICES]\nP 4 -2\n",
        )
        .unwrap();
        let mut g = Geometry::default();
        g.refresh(&net, 1, 1);
        assert_eq!(g.bounds, Some([0.0, -2.0, 10.0, 5.0]));
        assert_eq!(g.ends, vec![Some((0, 1))]);
        assert_eq!(g.boxes, vec![Some([0.0, -2.0, 10.0, 5.0])]);
        net.nodes[1].pos = Some([20.0, 5.0]);
        g.refresh(&net, 1, 1);
        assert_eq!(g.bounds.unwrap()[2], 10.0, "same key: cached");
        g.mark_moved();
        g.refresh(&net, 1, 1);
        assert_eq!(g.bounds.unwrap()[2], 20.0);
    }

    #[test]
    fn occupancy_claims_once() {
        let mut o = Occupancy::new(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0)),
            2.0,
        );
        assert!(o.claim(egui::pos2(3.0, 3.0)));
        assert!(!o.claim(egui::pos2(3.5, 2.5)));
        assert!(o.claim(egui::pos2(5.0, 3.0)));
        assert!(o.claim(egui::pos2(-50.0, 3.0)), "off-grid is always drawn");
    }
}
