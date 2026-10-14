//! Builds network objects from imported points and lines (EPANET-UI's
//! `projectbuilder` and the loaders behind its importers): nodes from point
//! records or from the ends of lines, merged when closer than a snap
//! tolerance; links with their interior vertices; properties from attributes.
//! Used by the Shapefile Importer (and meant for the DXF and CSV ones).

use std::collections::HashMap;

use eui_core::props::{Element, PropKey};
use eui_core::{LinkKind, Network, NodeKind};

use crate::setup::SetupDefaults;

/// A node to import; values are already in the project's units.
#[derive(Clone, Debug, Default)]
pub struct InNode {
    pub id: Option<String>,
    pub kind: Option<NodeKind>,
    pub xy: [f64; 2],
    pub elevation: Option<f64>,
    pub demand: Option<f64>,
    pub description: Option<String>,
    pub tag: Option<String>,
}

/// A link to import; values are already in the project's units.
#[derive(Clone, Debug, Default)]
pub struct InLink {
    pub id: Option<String>,
    pub kind: Option<LinkKind>,
    pub points: Vec<[f64; 2]>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub length: Option<f64>,
    pub diameter: Option<f64>,
    pub roughness: Option<f64>,
    pub description: Option<String>,
    pub tag: Option<String>,
}

/// Units of the map coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapUnits {
    None,
    Feet,
    Meters,
    Degrees,
}

impl MapUnits {
    pub const ALL: [MapUnits; 4] = [
        MapUnits::None,
        MapUnits::Feet,
        MapUnits::Meters,
        MapUnits::Degrees,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MapUnits::None => "None",
            MapUnits::Feet => "Feet",
            MapUnits::Meters => "Meters",
            MapUnits::Degrees => "Degrees",
        }
    }

    /// A point in meters, for distances (degrees through Web Mercator, as
    /// EPANET-UI measures snapping distances).
    fn metric(self, [x, y]: [f64; 2]) -> [f64; 2] {
        match self {
            MapUnits::Feet => [x * 0.3048, y * 0.3048],
            MapUnits::Degrees => {
                const R: f64 = 6378137.0;
                let lat = y.clamp(-85.0, 85.0).to_radians();
                [R * x.to_radians(), R * lat.tan().asinh()]
            }
            _ => [x, y],
        }
    }

    /// The true length in meters of a line in these units.
    fn length_m(self, pts: &[[f64; 2]]) -> f64 {
        pts.windows(2)
            .map(|w| match self {
                MapUnits::Degrees => haversine(w[0], w[1]),
                _ => {
                    let (a, b) = (self.metric(w[0]), self.metric(w[1]));
                    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
                }
            })
            .sum()
    }
}

fn haversine([lon1, lat1]: [f64; 2], [lon2, lat2]: [f64; 2]) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = (p2 - p1, (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371008.8 * a.sqrt().asin()
}

pub struct BuildOptions {
    /// Coordinate units (after any reprojection).
    pub units: MapUnits,
    /// Ends closer than this (meters) share a node.
    pub snap_m: f64,
    /// Lengths from the coordinates when no length attribute is given.
    pub compute_lengths: bool,
}

/// What an import did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub nodes_added: usize,
    pub nodes_updated: usize,
    pub links_added: usize,
    pub links_updated: usize,
    pub skipped: usize,
}

/// Nodes on a grid of snap-sized cells, for finding the nearest one quickly.
struct Snapper {
    cell: f64,
    tol: f64,
    units: MapUnits,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Snapper {
    fn key(&self, m: [f64; 2]) -> (i64, i64) {
        (
            (m[0] / self.cell).floor() as i64,
            (m[1] / self.cell).floor() as i64,
        )
    }
    fn insert(&mut self, i: usize, xy: [f64; 2]) {
        let k = self.key(self.units.metric(xy));
        self.grid.entry(k).or_default().push(i);
    }
    /// The nearest node within the tolerance (Manhattan distance, as EPANET-UI).
    fn nearest(&self, net: &Network, xy: [f64; 2]) -> Option<usize> {
        let m = self.units.metric(xy);
        let (kx, ky) = self.key(m);
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in self.grid.get(&(kx + dx, ky + dy)).into_iter().flatten() {
                    let Some(p) = net.nodes.get(i).and_then(|n| n.pos) else {
                        continue;
                    };
                    let q = self.units.metric(p);
                    let d = (q[0] - m[0]).abs() + (q[1] - m[1]).abs();
                    if d <= self.tol && best.is_none_or(|b| d < b.0) {
                        best = Some((d, i));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }
}

fn set(net: &mut Network, e: Element, key: PropKey, v: impl ToString) {
    let _ = net.set_prop(e, key, &v.to_string());
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Adds the nodes and links to `net`.
pub fn build(
    net: &mut Network,
    setup: &SetupDefaults,
    nodes: &[InNode],
    links: &[InLink],
    opt: &BuildOptions,
) -> Summary {
    let mut sum = Summary::default();
    let tol = opt.snap_m.max(0.0) + 1e-4;
    let mut snap = Snapper {
        cell: tol.max(1e-3),
        tol,
        units: opt.units,
        grid: HashMap::new(),
    };
    for (i, n) in net.nodes.iter().enumerate() {
        if let Some(p) = n.pos {
            snap.insert(i, p);
        }
    }
    let si = net.is_si();

    // point records
    for n in nodes {
        let existing = n.id.as_deref().and_then(|id| net.node_index(id));
        let i = match existing {
            Some(i) => {
                sum.nodes_updated += 1;
                net.nodes[i].pos = Some(n.xy);
                i
            }
            None => {
                let i = setup.add_node(net, n.kind.unwrap_or(NodeKind::Junction), n.xy);
                if let Some(id) = &n.id {
                    net.rename_node(i, id);
                }
                sum.nodes_added += 1;
                i
            }
        };
        snap.insert(i, n.xy);
        let e = Element::Node(i);
        if let Some(v) = n.elevation {
            set(net, e, PropKey::Field(0), fmt(v));
        }
        if let Some(v) = n.demand
            && net.nodes[i].kind == NodeKind::Junction
        {
            set(net, e, PropKey::Field(1), fmt(v));
        }
        if let Some(d) = &n.description {
            set(net, e, PropKey::Description, d);
        }
        if let Some(t) = &n.tag {
            set(net, e, PropKey::Tag, t);
        }
    }

    // line records
    for l in links {
        if l.points.len() < 2 {
            sum.skipped += 1;
            continue;
        }
        let existing = l.id.as_deref().and_then(|id| net.link_index(id));
        let i = match existing {
            Some(i) => {
                sum.links_updated += 1;
                i
            }
            None => {
                // the end nodes: named by attributes, else the nearest within
                // the snap tolerance, else new junctions
                let mut end = |name: &Option<String>, xy: [f64; 2], net: &mut Network| -> usize {
                    if let Some(i) = name.as_deref().and_then(|id| net.node_index(id)) {
                        return i;
                    }
                    if let Some(i) = snap.nearest(net, xy) {
                        return i;
                    }
                    let i = setup.add_node(net, NodeKind::Junction, xy);
                    if let Some(id) = name {
                        net.rename_node(i, id);
                    }
                    snap.insert(i, xy);
                    sum.nodes_added += 1;
                    i
                };
                let a = end(&l.start, l.points[0], net);
                let b = end(&l.end, *l.points.last().unwrap(), net);
                if a == b {
                    sum.skipped += 1;
                    continue;
                }
                let kind = l.kind.unwrap_or(LinkKind::Pipe);
                let i = setup.add_link(net, kind, a, b, false);
                if kind == LinkKind::Valve {
                    net.links[i].fields[1] = "TCV".into();
                }
                if let Some(id) = &l.id {
                    net.rename_link(i, id);
                }
                sum.links_added += 1;
                i
            }
        };
        // interior vertices, without repeats
        let mut verts: Vec<[f64; 2]> = Vec::new();
        for &p in &l.points[1..l.points.len() - 1] {
            if verts.last() != Some(&p) && p != l.points[0] {
                verts.push(p);
            }
        }
        net.links[i].vertices = verts;
        let e = Element::Link(i);
        if net.links[i].kind == LinkKind::Pipe {
            let length = l.length.filter(|v| *v > 0.0).or_else(|| {
                opt.compute_lengths.then(|| {
                    let m = opt.units.length_m(&l.points);
                    if si { m } else { m / 0.3048 }
                })
            });
            if let Some(v) = length.filter(|v| *v > 0.0) {
                set(net, e, PropKey::Field(0), fmt(v));
            }
            if let Some(v) = l.diameter.filter(|v| *v > 0.0) {
                set(net, e, PropKey::Field(1), fmt(v));
            }
            if let Some(v) = l.roughness.filter(|v| *v > 0.0) {
                set(net, e, PropKey::Field(2), fmt(v));
            }
        }
        if let Some(d) = &l.description {
            set(net, e, PropKey::Description, d);
        }
        if let Some(t) = &l.tag {
            set(net, e, PropKey::Tag, t);
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(pts: &[[f64; 2]]) -> InLink {
        InLink {
            points: pts.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn snaps_ends_and_builds_links() {
        let mut net = Network::new();
        let setup = SetupDefaults::default();
        let links = [
            line(&[[0.0, 0.0], [50.0, 10.0], [100.0, 0.0]]),
            // starts 0.3 m from the first line's end: same node with 0.5 m snap
            line(&[[100.3, 0.0], [200.0, 0.0]]),
            InLink {
                id: Some("P9".into()),
                diameter: Some(150.0),
                ..line(&[[200.0, 0.0], [200.0, 100.0]])
            },
        ];
        let opt = BuildOptions {
            units: MapUnits::Meters,
            snap_m: 0.5,
            compute_lengths: true,
        };
        let s = build(&mut net, &setup, &[], &links, &opt);
        assert_eq!(s.links_added, 3);
        assert_eq!(s.nodes_added, 4, "ends merged within the tolerance");
        assert_eq!(net.links[0].vertices, vec![[50.0, 10.0]]);
        let p9 = &net.links[net.link_index("P9").unwrap()];
        assert_eq!(p9.field(1), "150");
        // a new project is in SI units (LPS): 100 m
        assert!(net.is_si());
        assert_eq!(p9.field(0), "100");
    }

    #[test]
    fn point_records_name_nodes_and_degrees_snap_in_meters() {
        let mut net = Network::new();
        let setup = SetupDefaults::default();
        let nodes = [InNode {
            id: Some("R1".into()),
            kind: Some(NodeKind::Reservoir),
            xy: [33.0, 35.0],
            elevation: Some(120.0),
            ..Default::default()
        }];
        // about 0.5 m away in longitude
        let links = [line(&[[33.000005, 35.0], [33.001, 35.0]])];
        let opt = BuildOptions {
            units: MapUnits::Degrees,
            snap_m: 1.0,
            compute_lengths: true,
        };
        build(&mut net, &setup, &nodes, &links, &opt);
        let r = net.node_index("R1").unwrap();
        assert_eq!(net.nodes[r].kind, NodeKind::Reservoir);
        assert_eq!(net.links[0].from, "R1");
        let len: f64 = net.links[0].field(0).parse().unwrap();
        // 0.000995 degrees of longitude at 35 N is about 90.6 m
        assert!((len - 90.6).abs() < 0.5, "{len} m");
    }
}
