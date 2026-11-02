//! Map > Styling: QGIS-like symbology for the nodes (junctions, reservoirs
//! and tanks) and the links (pipes, pumps and valves), beside EPANET-UI's
//! map themes. Each layer's style is drawn with one of four renderers:
//!
//! - EPANET-UI Theme: the map theme and its legend, as in EPANET-UI;
//! - Single Symbol: one color and size for every node or link;
//! - Categorized: a color and size per node or link type;
//! - Graduated: the layer theme's values in classes (Equal Interval,
//!   Quantile, Natural Breaks or Pretty Breaks), colored along a color ramp,
//!   with sizes fixed, by class or proportional to the value.
//!
//! A node's size is the radius of its symbol, a link's its line width.
//! Styles are kept in the preferences and can be saved to a `.wstyle` file
//! (JSON) and loaded again, for other networks.

use egui::{Color32, RichText, Ui, vec2};
use eui_core::{LinkKind, NodeKind};
use serde::{Deserialize, Serialize};

use crate::app::{Dialog, EpanetApp};
use crate::i18n::{tr, trf};
use crate::style::{LinkTheme, NodeTheme};

/// The map layer a style is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    Nodes,
    #[default]
    Links,
}

impl Layer {
    pub fn label(self) -> &'static str {
        match self {
            Layer::Nodes => "Nodes",
            Layer::Links => "Links",
        }
    }
}

/// The theme a graduated style's classes were found for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Classified {
    Node(NodeTheme),
    Link(LinkTheme),
}

/// File extension of saved styles.
pub const EXT: &str = "wstyle";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Renderer {
    /// EPANET-UI's map theme and legend.
    #[default]
    Theme,
    Single,
    Categorized,
    Graduated,
}

impl Renderer {
    pub const ALL: [Renderer; 4] = [
        Renderer::Theme,
        Renderer::Single,
        Renderer::Categorized,
        Renderer::Graduated,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Renderer::Theme => "EPANET-UI Theme",
            Renderer::Single => "Single Symbol",
            Renderer::Categorized => "Categorized by Type",
            Renderer::Graduated => "Graduated",
        }
    }
}

/// How the class bounds of a graduated style are found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    EqualInterval,
    #[default]
    Quantile,
    NaturalBreaks,
    Pretty,
}

impl Method {
    pub const ALL: [Method; 4] = [
        Method::EqualInterval,
        Method::Quantile,
        Method::NaturalBreaks,
        Method::Pretty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Method::EqualInterval => "Equal Interval",
            Method::Quantile => "Quantile (Equal Count)",
            Method::NaturalBreaks => "Natural Breaks (Jenks)",
            Method::Pretty => "Pretty Breaks",
        }
    }
}

/// How wide the links of a graduated style are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidthBy {
    Fixed,
    #[default]
    Class,
    Value,
}

impl WidthBy {
    pub const ALL: [WidthBy; 3] = [WidthBy::Fixed, WidthBy::Class, WidthBy::Value];

    pub fn label(self) -> &'static str {
        match self {
            WidthBy::Fixed => "Fixed",
            WidthBy::Class => "By Class",
            WidthBy::Value => "Proportional to Value",
        }
    }
}

/// Color ramps, lowest value first (matplotlib's and ColorBrewer's, as QGIS
/// offers them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ramp {
    #[default]
    Viridis,
    Magma,
    Plasma,
    Inferno,
    Cividis,
    Turbo,
    Blues,
    Greens,
    Reds,
    YlOrRd,
    YlGnBu,
    RdYlBu,
    RdYlGn,
    Spectral,
    /// EPANET-UI's five legend colors.
    Epanet,
}

impl Ramp {
    pub const ALL: [Ramp; 15] = [
        Ramp::Viridis,
        Ramp::Magma,
        Ramp::Plasma,
        Ramp::Inferno,
        Ramp::Cividis,
        Ramp::Turbo,
        Ramp::Blues,
        Ramp::Greens,
        Ramp::Reds,
        Ramp::YlOrRd,
        Ramp::YlGnBu,
        Ramp::RdYlBu,
        Ramp::RdYlGn,
        Ramp::Spectral,
        Ramp::Epanet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Ramp::Viridis => "Viridis",
            Ramp::Magma => "Magma",
            Ramp::Plasma => "Plasma",
            Ramp::Inferno => "Inferno",
            Ramp::Cividis => "Cividis",
            Ramp::Turbo => "Turbo",
            Ramp::Blues => "Blues",
            Ramp::Greens => "Greens",
            Ramp::Reds => "Reds",
            Ramp::YlOrRd => "YlOrRd",
            Ramp::YlGnBu => "YlGnBu",
            Ramp::RdYlBu => "RdYlBu",
            Ramp::RdYlGn => "RdYlGn",
            Ramp::Spectral => "Spectral",
            Ramp::Epanet => "EPANET-UI",
        }
    }

    fn stops(self) -> &'static [u32] {
        match self {
            Ramp::Viridis => &[
                0x440154, 0x482878, 0x3e4989, 0x31688e, 0x26828e, 0x1f9e89, 0x35b779, 0x6ece58,
                0xb5de2b, 0xfde725,
            ],
            Ramp::Magma => &[
                0x000004, 0x1c1044, 0x4f127b, 0x812581, 0xb5367a, 0xe55964, 0xfb8761, 0xfec287,
                0xfcfdbf,
            ],
            Ramp::Plasma => &[
                0x0d0887, 0x4b03a1, 0x7d03a8, 0xa82296, 0xcb4679, 0xe56b5d, 0xf89441, 0xfdc328,
                0xf0f921,
            ],
            Ramp::Inferno => &[
                0x000004, 0x1b0c41, 0x4a0c6b, 0x781c6d, 0xa52c60, 0xcf4446, 0xed6925, 0xfb9b06,
                0xf7d13d, 0xfcffa4,
            ],
            Ramp::Cividis => &[
                0x00224e, 0x123570, 0x3b496c, 0x575d6d, 0x707173, 0x8a8779, 0xa69d75, 0xc4b56c,
                0xe4cf5b, 0xfee838,
            ],
            Ramp::Turbo => &[
                0x30123b, 0x4662d7, 0x36aaf9, 0x1ae4b6, 0x72fe5e, 0xc8ef34, 0xfaba39, 0xf66b19,
                0xca2a04, 0x7a0403,
            ],
            Ramp::Blues => &[
                0xdeebf7, 0xc6dbef, 0x9ecae1, 0x6baed6, 0x4292c6, 0x2171b5, 0x08519c, 0x08306b,
            ],
            Ramp::Greens => &[
                0xe5f5e0, 0xc7e9c0, 0xa1d99b, 0x74c476, 0x41ab5d, 0x238b45, 0x006d2c, 0x00441b,
            ],
            Ramp::Reds => &[
                0xfee0d2, 0xfcbba1, 0xfc9272, 0xfb6a4a, 0xef3b2c, 0xcb181d, 0xa50f15, 0x67000d,
            ],
            Ramp::YlOrRd => &[
                0xffeda0, 0xfed976, 0xfeb24c, 0xfd8d3c, 0xfc4e2a, 0xe31a1c, 0xbd0026, 0x800026,
            ],
            Ramp::YlGnBu => &[
                0xedf8b1, 0xc7e9b4, 0x7fcdbb, 0x41b6c4, 0x1d91c0, 0x225ea8, 0x253494, 0x081d58,
            ],
            Ramp::RdYlBu => &[
                0xa50026, 0xd73027, 0xf46d43, 0xfdae61, 0xfee090, 0xffffbf, 0xe0f3f8, 0xabd9e9,
                0x74add1, 0x4575b4, 0x313695,
            ],
            Ramp::RdYlGn => &[
                0xa50026, 0xd73027, 0xf46d43, 0xfdae61, 0xfee08b, 0xffffbf, 0xd9ef8b, 0xa6d96a,
                0x66bd63, 0x1a9850, 0x006837,
            ],
            Ramp::Spectral => &[
                0x9e0142, 0xd53e4f, 0xf46d43, 0xfdae61, 0xfee08b, 0xffffbf, 0xe6f598, 0xabdda4,
                0x66c2a5, 0x3288bd, 0x5e4fa2,
            ],
            Ramp::Epanet => &[0x7092be, 0x99d9ea, 0xb5e61d, 0xffc90e, 0xff7f27],
        }
    }

    /// The ramp's color at `t` (0 to 1).
    pub fn at(self, t: f32) -> Color32 {
        let s = self.stops();
        let x = t.clamp(0.0, 1.0) * (s.len() - 1) as f32;
        let k = (x.floor() as usize).min(s.len() - 2);
        let f = x - k as f32;
        let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8].map(|v| v as f32);
        let (a, b) = (rgb(s[k]), rgb(s[k + 1]));
        let mix = |i: usize| (a[i] + (b[i] - a[i]) * f).round() as u8;
        Color32::from_rgb(mix(0), mix(1), mix(2))
    }
}

/// The color and size a node or link is drawn with: a link's line width, a
/// node's symbol radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Symbol {
    pub color: Color32,
    pub width: f32,
}

/// The style of the nodes or of the links (Map > Styling). Widths are a
/// link's line width and a node's symbol radius.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub renderer: Renderer,
    /// Single Symbol.
    pub color: [u8; 3],
    pub width: f32,
    /// Categorized: pipes, pumps and valves, or junctions, reservoirs and
    /// tanks.
    pub kind_colors: [[u8; 3]; 3],
    pub kind_widths: [f32; 3],
    /// Graduated.
    pub classes: usize,
    pub method: Method,
    pub ramp: Ramp,
    pub invert: bool,
    pub width_by: WidthBy,
    pub min_width: f32,
    pub max_width: f32,
    /// Class bounds, lowest first: one more than there are classes.
    pub bounds: Vec<f32>,
    /// The classes' colors.
    pub colors: Vec<[u8; 3]>,
    /// The theme the classes were found for.
    pub classified: Option<Classified>,
    /// Values are classified with their sign (node values: a negative
    /// pressure is low), else by magnitude (link flows run either way).
    pub signed: bool,
    /// Elements without a value (a pump has no diameter).
    pub no_value: [u8; 3],
    /// 0.1 (nearly see-through) to 1.
    pub opacity: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style::links()
    }
}

impl Style {
    /// The links' style at first.
    pub fn links() -> Self {
        Style {
            renderer: Renderer::Theme,
            color: [0x1f, 0x5f, 0xa8],
            width: 2.0,
            kind_colors: [[0x1f, 0x5f, 0xa8], [0xd9, 0x48, 0x01], [0x2c, 0xa2, 0x5f]],
            kind_widths: [2.0, 4.0, 4.0],
            classes: 5,
            method: Method::Quantile,
            ramp: Ramp::Viridis,
            invert: false,
            width_by: WidthBy::Class,
            min_width: 1.0,
            max_width: 6.0,
            bounds: Vec::new(),
            colors: Vec::new(),
            classified: None,
            signed: false,
            no_value: [0xa0, 0xa0, 0xa0],
            opacity: 1.0,
        }
    }

    /// The nodes' style at first.
    pub fn nodes() -> Self {
        Style {
            color: [0x1f, 0x5f, 0xa8],
            width: 3.0,
            kind_colors: [[0x1f, 0x5f, 0xa8], [0x2c, 0xa2, 0x5f], [0xd9, 0x48, 0x01]],
            kind_widths: [3.0, 5.0, 5.0],
            width_by: WidthBy::Fixed,
            min_width: 2.0,
            max_width: 7.0,
            signed: true,
            ..Style::links()
        }
    }
}

fn rgb([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn to_rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

/// The Categorized row of a link type.
pub fn link_kind(kind: LinkKind) -> usize {
    match kind {
        LinkKind::Pipe => 0,
        LinkKind::Pump => 1,
        LinkKind::Valve => 2,
    }
}

/// The Categorized row of a node type.
pub fn node_kind(kind: NodeKind) -> usize {
    match kind {
        NodeKind::Junction => 0,
        NodeKind::Reservoir => 1,
        NodeKind::Tank => 2,
    }
}

/// The Categorized rows' names.
fn kind_names(layer: Layer) -> [&'static str; 3] {
    match layer {
        Layer::Nodes => ["Junctions", "Reservoirs", "Tanks"],
        Layer::Links => ["Pipes", "Pumps", "Valves"],
    }
}

impl Style {
    /// Whether the style, not EPANET-UI's theme, colors the layer.
    pub fn active(&self) -> bool {
        self.renderer != Renderer::Theme
    }

    /// Number of graduated classes there are bounds for.
    pub fn class_count(&self) -> usize {
        self.bounds.len().saturating_sub(1)
    }

    /// The class `value` falls in (values beyond the bounds in the end ones).
    pub fn class_of(&self, value: f32) -> Option<usize> {
        let n = self.class_count();
        if n == 0 {
            return None;
        }
        let v = self.magnitude(value);
        Some(
            self.bounds[1..n]
                .iter()
                .position(|&b| v < b)
                .unwrap_or(n - 1),
        )
    }

    /// The value as it is classified: signed, or its magnitude.
    fn magnitude(&self, v: f32) -> f32 {
        if self.signed { v } else { v.abs() }
    }

    fn class_color(&self, k: usize) -> Color32 {
        self.colors
            .get(k)
            .copied()
            .map_or_else(|| self.ramp_color(k, self.class_count()), rgb)
    }

    fn ramp_color(&self, k: usize, n: usize) -> Color32 {
        let t = if n <= 1 {
            0.5
        } else {
            k as f32 / (n - 1) as f32
        };
        self.ramp.at(if self.invert { 1.0 - t } else { t })
    }

    fn class_width(&self, k: usize) -> f32 {
        let n = self.class_count();
        match self.width_by {
            WidthBy::Fixed => self.width,
            _ if n <= 1 => self.max_width,
            _ => self.min_width + (self.max_width - self.min_width) * k as f32 / (n - 1) as f32,
        }
    }

    /// How an element of the Categorized row `kind` (see [`link_kind`],
    /// [`node_kind`]) with the theme's `value` is drawn; None for EPANET-UI's
    /// theme.
    pub fn symbol(&self, kind: usize, value: Option<f32>) -> Option<Symbol> {
        let fade = |c: Color32| {
            if self.opacity < 1.0 {
                c.gamma_multiply(self.opacity.clamp(0.05, 1.0))
            } else {
                c
            }
        };
        let s = match self.renderer {
            Renderer::Theme => return None,
            Renderer::Single => Symbol {
                color: rgb(self.color),
                width: self.width,
            },
            Renderer::Categorized => {
                let k = kind.min(2);
                Symbol {
                    color: rgb(self.kind_colors[k]),
                    width: self.kind_widths[k],
                }
            }
            Renderer::Graduated => match value.and_then(|v| self.class_of(v).map(|k| (v, k))) {
                None => Symbol {
                    color: rgb(self.no_value),
                    width: self.min_width.min(self.width),
                },
                Some((v, k)) => Symbol {
                    color: self.class_color(k),
                    width: if self.width_by == WidthBy::Value {
                        let (lo, hi) = (self.bounds[0], self.bounds[self.class_count()]);
                        let t = if hi > lo {
                            ((self.magnitude(v) - lo) / (hi - lo)).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        self.min_width + (self.max_width - self.min_width) * t
                    } else {
                        self.class_width(k)
                    },
                },
            },
        };
        Some(Symbol {
            color: fade(s.color),
            width: s.width.max(0.5),
        })
    }

    /// Finds the class bounds for `values` (signed or their magnitudes) and
    /// colors the classes along the ramp. With no values the classes are
    /// cleared.
    pub fn classify(&mut self, values: &[f32]) {
        let mut v: Vec<f64> = values
            .iter()
            .filter(|x| x.is_finite())
            .map(|&x| self.magnitude(x) as f64)
            .collect();
        v.sort_by(f64::total_cmp);
        self.bounds = class_bounds(&v, self.classes.clamp(2, 12), self.method)
            .into_iter()
            .map(|b| b as f32)
            .collect();
        self.recolor();
    }

    /// Colors the classes along the ramp again.
    pub fn recolor(&mut self) {
        let n = self.class_count();
        self.colors = (0..n).map(|k| to_rgb(self.ramp_color(k, n))).collect();
    }

    /// The legend of the style of `layer`; None for EPANET-UI's theme.
    pub fn legend(&self, title: String, layer: Layer) -> Option<crate::map::LegendBox> {
        use crate::map::{LegendBox, Swatch};
        let swatch = |s: Symbol| match layer {
            Layer::Nodes => Swatch::Dot {
                color: s.color,
                radius: s.width,
            },
            Layer::Links => Swatch::Line {
                color: s.color,
                width: s.width,
            },
        };
        let rows = match self.renderer {
            Renderer::Theme => return None,
            Renderer::Single => {
                vec![(swatch(self.symbol(0, None)?), tr(layer.label()).to_string())]
            }
            Renderer::Categorized => kind_names(layer)
                .into_iter()
                .enumerate()
                .map(|(k, label)| (swatch(self.symbol(k, None).unwrap()), tr(label).to_string()))
                .collect(),
            Renderer::Graduated => {
                let n = self.class_count();
                if n == 0 {
                    return None;
                }
                (0..n)
                    .map(|k| {
                        let (a, b) = (self.bounds[k], self.bounds[k + 1]);
                        // the middle of the class shows its width
                        let mid = if self.width_by == WidthBy::Value {
                            (a + b) / 2.0
                        } else {
                            a
                        };
                        let mut s = self.symbol(0, Some(mid)).unwrap();
                        s.color = {
                            let c = self.class_color(k);
                            if self.opacity < 1.0 {
                                c.gamma_multiply(self.opacity)
                            } else {
                                c
                            }
                        };
                        let label = format!(
                            "{} - {}",
                            crate::style::fmt_break(a),
                            crate::style::fmt_break(b)
                        );
                        (swatch(s), label)
                    })
                    .collect()
            }
        };
        Some(LegendBox {
            title: match (self.renderer, layer) {
                (Renderer::Categorized, Layer::Nodes) => tr("Node Type").to_string(),
                (Renderer::Categorized, Layer::Links) => tr("Link Type").to_string(),
                _ => title,
            },
            rows,
        })
    }
}

/// Class bounds (one more than the classes) of sorted values.
fn class_bounds(v: &[f64], n: usize, method: Method) -> Vec<f64> {
    let (Some(&lo), Some(&hi)) = (v.first(), v.last()) else {
        return Vec::new();
    };
    if hi <= lo {
        return vec![lo, hi];
    }
    let mut b: Vec<f64> = match method {
        Method::EqualInterval => (0..=n)
            .map(|k| lo + (hi - lo) * k as f64 / n as f64)
            .collect(),
        Method::Quantile => (0..=n).map(|k| v[((v.len() - 1) * k) / n]).collect(),
        Method::NaturalBreaks => jenks(v, n),
        Method::Pretty => {
            let step = nice((hi - lo) / n as f64);
            let start = (lo / step).floor() * step;
            let mut out = vec![start];
            while *out.last().unwrap() < hi - step * 1e-9 {
                out.push(start + step * out.len() as f64);
            }
            out
        }
    };
    b[0] = lo.min(b[0]);
    let last = b.len() - 1;
    b[last] = hi.max(b[last]);
    // equal bounds (many equal values) leave empty classes out
    b.dedup_by(|x, y| (*x - *y).abs() <= f64::EPSILON * y.abs().max(1.0));
    if b.len() < 2 {
        b = vec![lo, hi];
    }
    b
}

/// A round step near `x`: 1, 2, 2.5 or 5 times a power of ten.
fn nice(x: f64) -> f64 {
    let p = 10f64.powf(x.log10().floor());
    let f = x / p;
    p * if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 2.5 {
        2.5
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    }
}

/// Jenks natural breaks (Fisher's optimal classes) of sorted values, on an
/// even sample of at most 1500 of them.
fn jenks(all: &[f64], k: usize) -> Vec<f64> {
    const MAX: usize = 1500;
    let v: Vec<f64> = if all.len() > MAX {
        (0..MAX)
            .map(|i| all[i * (all.len() - 1) / (MAX - 1)])
            .collect()
    } else {
        all.to_vec()
    };
    let n = v.len();
    let k = k.min(n);
    if k < 2 {
        return vec![v[0], v[n - 1]];
    }
    // lower[l][j]: first value of the last class when the first l values are
    // split into j classes; var[l][j]: the least sum of squared deviations
    let mut lower = vec![vec![0usize; k + 1]; n + 1];
    let mut var = vec![vec![f64::INFINITY; k + 1]; n + 1];
    for j in 1..=k {
        lower[1][j] = 1;
        var[1][j] = 0.0;
    }
    for l in 2..=n {
        let (mut s1, mut s2, mut w) = (0.0, 0.0, 0.0);
        let mut vl = 0.0;
        for m in 1..=l {
            let i3 = l - m + 1;
            let val = v[i3 - 1];
            s2 += val * val;
            s1 += val;
            w += 1.0;
            vl = s2 - s1 * s1 / w;
            let i4 = i3 - 1;
            if i4 != 0 {
                for j in 2..=k {
                    if var[l][j] >= vl + var[i4][j - 1] {
                        lower[l][j] = i3;
                        var[l][j] = vl + var[i4][j - 1];
                    }
                }
            }
        }
        lower[l][1] = 1;
        var[l][1] = vl;
    }
    let mut b = vec![0.0; k + 1];
    b[k] = v[n - 1];
    b[0] = v[0];
    let mut l = n;
    for j in (2..=k).rev() {
        let id = lower[l][j].saturating_sub(1).max(1);
        b[j - 1] = v[id - 1].midpoint(v[id]);
        l = lower[l][j] - 1;
        if l == 0 {
            break;
        }
    }
    b
}

/// The style of a layer.
pub fn style(app: &EpanetApp, layer: Layer) -> &Style {
    match layer {
        Layer::Nodes => &app.prefs.node_style,
        Layer::Links => &app.prefs.link_style,
    }
}

fn style_mut(app: &mut EpanetApp, layer: Layer) -> &mut Style {
    match layer {
        Layer::Nodes => &mut app.prefs.node_style,
        Layer::Links => &mut app.prefs.link_style,
    }
}

/// The map theme of a layer.
fn theme(app: &EpanetApp, layer: Layer) -> Classified {
    match layer {
        Layer::Nodes => Classified::Node(app.prefs.node_theme),
        Layer::Links => Classified::Link(app.prefs.link_theme),
    }
}

fn set_theme(app: &mut EpanetApp, t: Classified) {
    match t {
        Classified::Node(t) => app.prefs.node_theme = t,
        Classified::Link(t) => app.prefs.link_theme = t,
    }
}

fn theme_name(app: &EpanetApp, t: Classified) -> String {
    match t {
        Classified::Node(t) => app.node_theme_name(t),
        Classified::Link(t) => app.link_theme_name(t),
    }
}

fn no_theme(t: Classified) -> bool {
    matches!(
        t,
        Classified::Node(NodeTheme::None) | Classified::Link(LinkTheme::None)
    )
}

/// The themes a graduated style can classify, with whether they have values
/// (result themes need an analysis).
fn themes(app: &EpanetApp, layer: Layer) -> Vec<(Classified, String, bool)> {
    let results = app.results.is_some();
    match layer {
        Layer::Nodes => app
            .node_themes()
            .into_iter()
            .filter(|&t| t != NodeTheme::None)
            .map(|t| {
                let c = Classified::Node(t);
                (c, theme_name(app, c), !t.is_result() || results)
            })
            .collect(),
        Layer::Links => app
            .link_themes()
            .into_iter()
            .filter(|&t| t != LinkTheme::None)
            .map(|t| {
                let c = Classified::Link(t);
                (c, theme_name(app, c), !t.is_result() || results)
            })
            .collect(),
    }
}

/// The theme value of the layer's element `i`.
fn value(app: &EpanetApp, layer: Layer, i: usize) -> Option<f32> {
    match layer {
        Layer::Nodes => app.node_theme_value(i),
        Layer::Links => app.link_theme_value(i),
    }
}

fn element_count(app: &EpanetApp, layer: Layer) -> usize {
    match layer {
        Layer::Nodes => app.net.nodes.len(),
        Layer::Links => app.net.links.len(),
    }
}

/// Classifies the graduated styles again when their theme changed, or when
/// they have no classes yet (as when result values were not there before).
pub fn prepare(app: &mut EpanetApp) {
    for layer in [Layer::Nodes, Layer::Links] {
        let s = style(app, layer);
        if s.renderer != Renderer::Graduated {
            continue;
        }
        let t = theme(app, layer);
        if s.classified == Some(t) && s.class_count() > 0 {
            continue;
        }
        classify(app, layer);
    }
}

/// Classifies the layer's theme values.
fn classify(app: &mut EpanetApp, layer: Layer) {
    let t = theme(app, layer);
    let values = theme_values(app, layer);
    let s = style_mut(app, layer);
    s.classify(&values);
    // tried again once there are values (results to come)
    s.classified = (!values.is_empty()).then_some(t);
}

fn theme_values(app: &EpanetApp, layer: Layer) -> Vec<f32> {
    (0..element_count(app, layer))
        .filter_map(|i| value(app, layer, i))
        .collect()
}

/// A layer legend's title for a graduated style: the theme and its units.
pub fn theme_title(app: &EpanetApp, layer: Layer) -> String {
    let t = theme(app, layer);
    let u = match t {
        Classified::Node(t) => app.node_theme_units(t),
        Classified::Link(t) => app.link_theme_units(t),
    };
    let name = theme_name(app, t);
    if u.is_empty() {
        name
    } else {
        format!("{name} ({u})")
    }
}

/// Opens Map > Styling for a layer.
pub fn open(app: &mut EpanetApp, layer: Layer) {
    app.styling_layer = layer;
    app.dialog = Dialog::Styling;
}

/// Loads a saved style, for the layer shown in the Styling window.
pub fn load(app: &mut EpanetApp, name: &str, bytes: &[u8]) {
    match serde_json::from_slice::<Style>(bytes) {
        Ok(s) => {
            let layer = app.styling_layer;
            *style_mut(app, layer) = s;
            app.status = trf("Style {} loaded", &[&name]);
        }
        Err(e) => app.status = format!("{name}: {e}"),
    }
}

fn save(app: &mut EpanetApp) {
    let Ok(json) = serde_json::to_string_pretty(style(app, app.styling_layer)) else {
        return;
    };
    let layer = match app.styling_layer {
        Layer::Nodes => "nodes",
        Layer::Links => "links",
    };
    let name = format!("{} {layer}.{EXT}", app.file_name);
    app.status = match crate::io::save_bytes(
        json.as_bytes(),
        &name,
        "WEPANET style",
        EXT,
        "application/json",
    ) {
        Ok(()) => trf("Saved {}", &[&name]),
        Err(e) => e,
    };
}

/// A color button (opens a color picker).
fn color_button(ui: &mut Ui, c: &mut [u8; 3]) -> bool {
    let mut color = rgb(*c);
    let changed = egui::widgets::color_picker::color_edit_button_srgba(
        ui,
        &mut color,
        egui::widgets::color_picker::Alpha::Opaque,
    )
    .changed();
    if changed {
        *c = to_rgb(color);
    }
    changed
}

fn width_drag(ui: &mut Ui, w: &mut f32) {
    ui.add(
        egui::DragValue::new(w)
            .range(0.5..=20.0)
            .speed(0.1)
            .fixed_decimals(1)
            .suffix(" px"),
    );
}

/// The ramp drawn as a bar.
fn ramp_bar(ui: &mut Ui, ramp: Ramp, invert: bool, size: egui::Vec2) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let steps = 40;
    for k in 0..steps {
        let t = k as f32 / (steps - 1) as f32;
        let x0 = r.left() + r.width() * k as f32 / steps as f32;
        let x1 = r.left() + r.width() * (k + 1) as f32 / steps as f32;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, r.top()), egui::pos2(x1 + 0.5, r.bottom())),
            0.0,
            ramp.at(if invert { 1.0 - t } else { t }),
        );
    }
    ui.painter().rect_stroke(
        r,
        0.0,
        egui::Stroke::new(1.0, Color32::from_gray(140)),
        egui::StrokeKind::Inside,
    );
    resp
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    all: &[T],
    label: impl Fn(T) -> &'static str,
) -> bool {
    let before = *value;
    egui::ComboBox::from_id_salt(id)
        .width(210.0)
        .selected_text(tr(label(*value)))
        .show_ui(ui, |ui| {
            for &v in all {
                ui.selectable_value(value, v, tr(label(v)));
            }
        });
    *value != before
}

/// "Width" of a link, "Size" of a node.
fn size_label(layer: Layer) -> &'static str {
    match layer {
        Layer::Nodes => "Size",
        Layer::Links => "Width",
    }
}

/// The Styling window: the layer and its renderer and settings above, Load,
/// Save, OK and Cancel below. Changes show on the map at once; Cancel
/// undoes them (of both layers).
pub fn panel(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    if app.prefs_backup.is_none() {
        app.prefs_backup = Some(app.prefs.clone());
    }
    ui.add_space(6.0);
    egui::Grid::new("styling_top")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("Layer"));
            ui.horizontal(|ui| {
                for l in [Layer::Nodes, Layer::Links] {
                    ui.selectable_value(&mut app.styling_layer, l, tr(l.label()));
                }
            });
            ui.end_row();
            let layer = app.styling_layer;
            ui.label(tr("Renderer"));
            let s = style_mut(app, layer);
            let was = s.renderer;
            combo(
                ui,
                "styling_renderer",
                &mut s.renderer,
                &Renderer::ALL,
                |r| r.label(),
            );
            // a style is shown with its legend
            if was == Renderer::Theme && s.renderer != Renderer::Theme {
                *legend_visible(app, layer) = true;
            }
            ui.end_row();
        });
    let layer = app.styling_layer;
    ui.separator();
    match style(app, layer).renderer {
        Renderer::Theme => {
            ui.label(
                RichText::new(tr(match layer {
                    Layer::Nodes => "The nodes are colored by the map theme chosen in the Map Viewer, with EPANET-UI's legend.",
                    Layer::Links => "The links are colored by the map theme chosen in the Map Viewer, with EPANET-UI's legend.",
                }))
                .weak(),
            );
        }
        Renderer::Single => {
            egui::Grid::new("styling_single")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    let s = style_mut(app, layer);
                    ui.label(tr("Color"));
                    color_button(ui, &mut s.color);
                    ui.end_row();
                    ui.label(tr(size_label(layer)));
                    width_drag(ui, &mut s.width);
                    ui.end_row();
                });
        }
        Renderer::Categorized => categorized(app, ui, layer),
        Renderer::Graduated => graduated(app, ui, layer),
    }
    if style(app, layer).active() {
        ui.separator();
        egui::Grid::new("styling_common")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("Opacity"));
                ui.add(
                    egui::Slider::new(&mut style_mut(app, layer).opacity, 0.1..=1.0)
                        .fixed_decimals(2),
                );
                ui.end_row();
                ui.label("");
                let border = match layer {
                    Layer::Nodes => &mut app.prefs.node_border,
                    Layer::Links => &mut app.prefs.link_border,
                };
                crate::widgets::checkbox(ui, border, "Display Border");
                ui.end_row();
                ui.label("");
                crate::widgets::checkbox(ui, legend_visible(app, layer), "Legend");
                ui.end_row();
            });
    }
    ui.add_space(12.0);
    let mut close = false;
    match crate::dialogs::buttons(ui, &["Load…", "Save…", "OK", "Cancel"]) {
        Some(0) => crate::io::open_file(&app.inbox, ui.ctx(), "WEPANET style", &[EXT]),
        Some(1) => save(app),
        Some(2) => {
            app.prefs_backup = None;
            close = true;
        }
        Some(3) => {
            cancel(app);
            close = true;
        }
        _ => {}
    }
    ui.add_space(6.0);
    close
}

fn legend_visible(app: &mut EpanetApp, layer: Layer) -> &mut bool {
    match layer {
        Layer::Nodes => &mut app.prefs.node_legend_visible,
        Layer::Links => &mut app.prefs.link_legend_visible,
    }
}

/// Undoes the changes made in the Styling window.
pub fn cancel(app: &mut EpanetApp) {
    if let Some(old) = app.prefs_backup.take() {
        app.prefs = old;
    }
}

fn categorized(app: &mut EpanetApp, ui: &mut Ui, layer: Layer) {
    let counts = match layer {
        Layer::Nodes => [NodeKind::Junction, NodeKind::Reservoir, NodeKind::Tank]
            .map(|k| app.net.nodes.iter().filter(|n| n.kind == k).count()),
        Layer::Links => [LinkKind::Pipe, LinkKind::Pump, LinkKind::Valve]
            .map(|k| app.net.links.iter().filter(|l| l.kind == k).count()),
    };
    egui::Grid::new("styling_categories")
        .num_columns(4)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(tr("Color")).weak());
            ui.label(RichText::new(tr(size_label(layer))).weak());
            ui.label(RichText::new(tr("Count")).weak());
            ui.end_row();
            let s = style_mut(app, layer);
            for (k, label) in kind_names(layer).iter().enumerate() {
                ui.label(tr(label));
                color_button(ui, &mut s.kind_colors[k]);
                width_drag(ui, &mut s.kind_widths[k]);
                ui.label(counts[k].to_string());
                ui.end_row();
            }
        });
}

fn graduated(app: &mut EpanetApp, ui: &mut Ui, layer: Layer) {
    let themes = themes(app, layer);
    let mut reclassify = false;
    egui::Grid::new("styling_graduated")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("Value"));
            let current = theme(app, layer);
            egui::ComboBox::from_id_salt(("styling_value", layer as u8))
                .width(210.0)
                .selected_text(if no_theme(current) {
                    tr("Choose a value").to_string()
                } else {
                    tr(&theme_name(app, current)).to_string()
                })
                .show_ui(ui, |ui| {
                    for (t, label, enabled) in &themes {
                        let resp = ui
                            .add_enabled(
                                *enabled,
                                egui::Button::selectable(current == *t, tr(label)),
                            )
                            .on_disabled_hover_text(tr("Run an analysis first"));
                        if resp.clicked() {
                            set_theme(app, *t);
                        }
                    }
                });
            ui.end_row();
            let s = style_mut(app, layer);
            ui.label(tr("Mode"));
            reclassify |= combo(ui, "styling_method", &mut s.method, &Method::ALL, |m| {
                m.label()
            });
            ui.end_row();
            ui.label(tr("Classes"));
            reclassify |= ui
                .add(egui::DragValue::new(&mut s.classes).range(2..=12))
                .changed();
            ui.end_row();
            ui.label(tr("Color Ramp"));
            ui.horizontal(|ui| {
                let before = s.ramp;
                egui::ComboBox::from_id_salt("styling_ramp")
                    .width(120.0)
                    .selected_text(s.ramp.label())
                    .show_ui(ui, |ui| {
                        for r in Ramp::ALL {
                            ui.horizontal(|ui| {
                                ramp_bar(ui, r, false, vec2(60.0, 12.0));
                                ui.selectable_value(&mut s.ramp, r, r.label());
                            });
                        }
                    });
                let flipped = ui.checkbox(&mut s.invert, tr("Invert")).changed();
                if s.ramp != before || flipped {
                    s.recolor();
                }
            });
            ui.end_row();
            ui.label("");
            ramp_bar(ui, s.ramp, s.invert, vec2(210.0, 12.0));
            ui.end_row();
            ui.label(tr(match layer {
                Layer::Nodes => "Symbol Size",
                Layer::Links => "Line Width",
            }));
            combo(
                ui,
                "styling_width_by",
                &mut s.width_by,
                &WidthBy::ALL,
                |w| w.label(),
            );
            ui.end_row();
            if s.width_by == WidthBy::Fixed {
                ui.label(tr(size_label(layer)));
                width_drag(ui, &mut s.width);
            } else {
                ui.label(tr("Min / Max"));
                ui.horizontal(|ui| {
                    width_drag(ui, &mut s.min_width);
                    width_drag(ui, &mut s.max_width);
                });
            }
            ui.end_row();
            ui.label(tr("No Value"));
            color_button(ui, &mut s.no_value);
            ui.end_row();
        });
    if reclassify || ui.button(tr("Classify")).clicked() {
        classify(app, layer);
    }
    // the classes: color, lower and upper bound, number of elements
    let s = style(app, layer);
    let n = s.class_count();
    if n == 0 {
        let none = no_theme(theme(app, layer));
        ui.label(
            RichText::new(tr(match (layer, none) {
                (Layer::Nodes, true) => "Choose the value to color the nodes by.",
                (Layer::Links, true) => "Choose the value to color the links by.",
                (Layer::Nodes, false) => {
                    "No node has a value to classify (run an analysis for results)."
                }
                (Layer::Links, false) => {
                    "No link has a value to classify (run an analysis for results)."
                }
            }))
            .weak(),
        );
        return;
    }
    let mut counts = vec![0usize; n];
    for i in 0..element_count(app, layer) {
        if let Some(k) = value(app, layer, i).and_then(|v| s.class_of(v)) {
            counts[k] += 1;
        }
    }
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .max_height(230.0)
        .id_salt("styling_classes")
        .show(ui, |ui| {
            egui::Grid::new("styling_class_rows")
                .num_columns(4)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    ui.label(RichText::new(tr("From")).weak());
                    ui.label(RichText::new(tr("To")).weak());
                    ui.label(RichText::new(tr("Count")).weak());
                    ui.end_row();
                    let s = style_mut(app, layer);
                    let span = (s.bounds[n] - s.bounds[0]).abs().max(1e-6);
                    for k in 0..n {
                        if s.colors.len() < n {
                            s.recolor();
                        }
                        color_button(ui, &mut s.colors[k]);
                        let lo = if k == 0 { f32::MIN } else { s.bounds[k - 1] };
                        let hi = if k + 1 == n {
                            f32::MAX
                        } else {
                            s.bounds[k + 2]
                        };
                        // the bounds between classes stay in order
                        let (a, rest) = s.bounds.split_at_mut(k + 1);
                        ui.add(
                            egui::DragValue::new(&mut a[k])
                                .speed(span / 300.0)
                                .range(lo..=rest[0]),
                        );
                        ui.add(
                            egui::DragValue::new(&mut rest[0])
                                .speed(span / 300.0)
                                .range(a[k]..=hi),
                        );
                        ui.label(counts[k].to_string());
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Vec<f32> {
        vec![1.0, 2.0, 2.0, 3.0, 10.0, 11.0, 12.0, 50.0, 51.0, 52.0]
    }

    #[test]
    fn equal_intervals_split_the_range() {
        let mut s = Style {
            method: Method::EqualInterval,
            classes: 4,
            ..Default::default()
        };
        s.classify(&values());
        assert_eq!(s.bounds, vec![1.0, 13.75, 26.5, 39.25, 52.0]);
        assert_eq!(s.colors.len(), 4);
        assert_eq!(s.class_of(1.0), Some(0));
        assert_eq!(s.class_of(52.0), Some(3));
        assert_eq!(s.class_of(-20.0), Some(1), "magnitudes are classified");
        assert_eq!(s.class_of(1000.0), Some(3));
    }

    #[test]
    fn quantiles_hold_about_as_many_values() {
        let mut s = Style {
            method: Method::Quantile,
            classes: 2,
            ..Default::default()
        };
        s.classify(&values());
        assert_eq!(s.bounds, vec![1.0, 10.0, 52.0]);
    }

    #[test]
    fn natural_breaks_find_the_groups() {
        let mut s = Style {
            method: Method::NaturalBreaks,
            classes: 3,
            ..Default::default()
        };
        s.classify(&values());
        assert_eq!(s.class_count(), 3);
        let class = |v| s.class_of(v).unwrap();
        assert_eq!(class(3.0), 0);
        assert_eq!(class(10.0), 1);
        assert_eq!(class(12.0), 1);
        assert_eq!(class(50.0), 2);
    }

    #[test]
    fn pretty_breaks_are_round() {
        let mut s = Style {
            method: Method::Pretty,
            classes: 5,
            ..Default::default()
        };
        s.classify(&values());
        assert_eq!(s.bounds[0], 0.0);
        assert!(s.bounds.iter().all(|b| b % 10.0 == 0.0), "{:?}", s.bounds);
        assert!(*s.bounds.last().unwrap() >= 52.0);
    }

    #[test]
    fn equal_values_make_one_class() {
        let mut s = Style::default();
        s.classify(&[4.0, 4.0, 4.0]);
        assert_eq!(s.class_count(), 1);
        s.classify(&[]);
        assert_eq!(s.class_count(), 0);
    }

    #[test]
    fn widths_follow_the_class_or_the_value() {
        let mut s = Style {
            renderer: Renderer::Graduated,
            method: Method::EqualInterval,
            classes: 4,
            min_width: 1.0,
            max_width: 7.0,
            ..Default::default()
        };
        s.classify(&values());
        let w = |s: &Style, v| s.symbol(0, Some(v)).unwrap().width;
        assert_eq!(w(&s, 1.0), 1.0);
        assert_eq!(w(&s, 52.0), 7.0);
        assert_eq!(w(&s, 20.0), 3.0);
        s.width_by = WidthBy::Value;
        assert_eq!(w(&s, 26.5), 4.0);
        // no value: the no-value color
        let none = s.symbol(1, None).unwrap();
        assert_eq!(none.color, rgb(s.no_value));
    }

    #[test]
    fn the_theme_renderer_leaves_the_links_to_the_theme() {
        let s = Style::default();
        assert!(!s.active());
        assert_eq!(s.symbol(0, Some(3.0)), None);
        assert!(s.legend("x".into(), Layer::Links).is_none());
    }

    #[test]
    fn categories_color_by_type() {
        let s = Style {
            renderer: Renderer::Categorized,
            ..Default::default()
        };
        let pump = s.symbol(1, None).unwrap();
        assert_eq!(pump.color, rgb(s.kind_colors[1]));
        assert_eq!(pump.width, s.kind_widths[1]);
        assert_eq!(s.legend("x".into(), Layer::Links).unwrap().rows.len(), 3);
    }

    #[test]
    fn ramps_run_from_end_to_end() {
        assert_eq!(Ramp::Viridis.at(0.0), Color32::from_rgb(0x44, 0x01, 0x54));
        assert_eq!(Ramp::Viridis.at(1.0), Color32::from_rgb(0xfd, 0xe7, 0x25));
        for r in Ramp::ALL {
            let _ = r.at(0.5);
        }
    }

    #[test]
    fn styles_save_and_load() {
        let mut s = Style {
            renderer: Renderer::Graduated,
            ramp: Ramp::Spectral,
            classified: Some(Classified::Link(LinkTheme::Diameter)),
            ..Default::default()
        };
        s.classify(&values());
        let json = serde_json::to_string(&s).unwrap();
        let back: Style = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        // an older or partial file takes the defaults for what it lacks
        let partial: Style = serde_json::from_str(r#"{"renderer":"Single"}"#).unwrap();
        assert_eq!(partial.renderer, Renderer::Single);
        assert_eq!(partial.width, Style::default().width);
    }

    /// The Styling window shows every renderer; Cancel undoes the changes.
    #[test]
    fn the_window_shows_every_renderer_and_cancel_undoes() {
        use eframe::App as _;
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.prefs.show_welcome = false;
        app.dialog = Dialog::None;
        let mut frame = eframe::Frame::_new_kittest();
        let mut run = |app: &mut EpanetApp| {
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    app.logic(ui.ctx(), &mut frame);
                    app.ui(ui, &mut frame);
                });
                out.textures_delta.clear();
            }
        };
        open(&mut app, Layer::Links);
        run(&mut app);
        assert_eq!(app.dialog, Dialog::Styling);
        for r in Renderer::ALL {
            app.prefs.link_style.renderer = r;
            run(&mut app);
        }
        assert!(
            app.prefs.link_style.class_count() >= 2,
            "Graduated classified"
        );
        // the nodes' page
        app.styling_layer = Layer::Nodes;
        app.prefs.node_theme = NodeTheme::Elevation;
        for r in Renderer::ALL {
            app.prefs.node_style.renderer = r;
            run(&mut app);
        }
        assert!(app.prefs.node_style.class_count() >= 2, "nodes classified");
        assert_eq!(
            app.prefs.node_style.classified,
            Some(Classified::Node(NodeTheme::Elevation))
        );
        cancel(&mut app);
        assert_eq!(app.prefs.link_style.renderer, Renderer::Theme);
        assert_eq!(app.prefs.node_style.renderer, Renderer::Theme);
    }

    /// The map draws the links in the style's color and width, and an SVG
    /// of the map keeps them.
    #[test]
    fn the_map_and_its_svg_follow_the_style() {
        use eframe::App as _;
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.prefs.link_style.renderer = Renderer::Single;
        app.prefs.link_style.color = [0x12, 0x34, 0x56];
        app.prefs.link_style.width = 5.0;
        app.prefs.link_legend_visible = true;
        let mut frame = eframe::Frame::_new_kittest();
        let mut shapes = Vec::new();
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx(), &mut frame);
                app.ui(ui, &mut frame);
            });
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        let map = app.export.map_rect.unwrap();
        let styled = shapes
            .iter()
            .filter(|c| match &c.shape {
                egui::Shape::Path(p) => {
                    p.stroke.width == 5.0
                        && matches!(p.stroke.color,
                            egui::epaint::ColorMode::Solid(c) if c == Color32::from_rgb(0x12, 0x34, 0x56))
                }
                _ => false,
            })
            .count();
        assert!(styled >= 12, "{styled} links in the style's color");
        let svg = crate::export::to_svg(&shapes, map);
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains(r##"stroke="#123456" stroke-width="5.00""##));
        // the legend's title and rows are text
        assert!(svg.contains(">Links</text>"), "no legend text");
    }

    /// The graduated style classifies the theme's values by itself, and
    /// again when the theme changes.
    #[test]
    fn graduated_styles_classify_their_theme() {
        let (_ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.prefs.link_style.renderer = Renderer::Graduated;
        app.prefs.link_theme = LinkTheme::Diameter;
        prepare(&mut app);
        let s = &app.prefs.link_style;
        assert_eq!(s.classified, Some(Classified::Link(LinkTheme::Diameter)));
        assert!(s.class_count() >= 2);
        app.prefs.link_theme = LinkTheme::Length;
        prepare(&mut app);
        assert_eq!(
            app.prefs.link_style.classified,
            Some(Classified::Link(LinkTheme::Length))
        );
        // no values yet (results): tried again later
        app.prefs.link_theme = LinkTheme::Flow;
        prepare(&mut app);
        assert_eq!(app.prefs.link_style.classified, None);
        assert_eq!(app.prefs.link_style.class_count(), 0);
    }

    /// Node values are classified with their sign: a negative pressure is
    /// the lowest class, not a high one.
    #[test]
    fn node_values_keep_their_sign() {
        let mut s = Style {
            renderer: Renderer::Graduated,
            method: Method::EqualInterval,
            classes: 2,
            ..Style::nodes()
        };
        s.classify(&[-40.0, 0.0, 10.0, 40.0]);
        assert_eq!(s.bounds, vec![-40.0, 0.0, 40.0]);
        assert_eq!(s.class_of(-30.0), Some(0));
        assert_eq!(s.class_of(30.0), Some(1));
        // the links' flows by magnitude
        let mut l = Style {
            method: Method::EqualInterval,
            classes: 2,
            ..Style::links()
        };
        l.classify(&[-40.0, 0.0, 10.0, 40.0]);
        assert_eq!(l.bounds, vec![0.0, 20.0, 40.0]);
    }

    /// Categorized nodes: a color and size per node type, with its legend
    /// of dots.
    #[test]
    fn nodes_are_categorized_by_type() {
        let s = Style {
            renderer: Renderer::Categorized,
            ..Style::nodes()
        };
        let tank = s.symbol(node_kind(NodeKind::Tank), None).unwrap();
        assert_eq!(tank.color, rgb(s.kind_colors[2]));
        assert_eq!(tank.width, s.kind_widths[2]);
        let legend = s.legend("x".into(), Layer::Nodes).unwrap();
        assert_eq!(legend.title, "Node Type");
        assert_eq!(legend.rows.len(), 3);
        assert!(matches!(legend.rows[0].0, crate::map::Swatch::Dot { .. }));
        assert_eq!(legend.rows[2].1, "Tanks");
    }

    /// The map draws the nodes in the node style's color and size.
    #[test]
    fn the_map_draws_the_nodes_in_their_style() {
        use eframe::App as _;
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.prefs.node_style.renderer = Renderer::Single;
        app.prefs.node_style.color = [0x65, 0x43, 0x21];
        app.prefs.node_style.width = 6.0;
        app.prefs.node_legend_visible = true;
        let mut frame = eframe::Frame::_new_kittest();
        let mut shapes = Vec::new();
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx(), &mut frame);
                app.ui(ui, &mut frame);
            });
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        let color = Color32::from_rgb(0x65, 0x43, 0x21);
        let dots = shapes
            .iter()
            .filter(|c| {
                matches!(&c.shape, egui::Shape::Circle(k) if k.radius == 6.0 && k.fill == color)
            })
            .count();
        // Net1's nine junctions
        assert!(dots >= 9, "{dots} junctions in the style's color");
        let svg = crate::export::to_svg(&shapes, app.export.map_rect.unwrap());
        assert!(svg.contains(">Nodes</text>"), "no node legend");
    }
}
