//! Map layers from web map services, drawn over the basemap and under the
//! network: WMS (GetMap) and ArcGIS REST map services (export). A layer is
//! asked for one picture of the area in view, in Web Mercator (or longitude/
//! latitude where a WMS has no Web Mercator), once the view has settled; the
//! picture goes through the project's coordinate transformation as the
//! basemap tiles do, so it lines up in any coordinate system, and the last
//! one stays in view while the next downloads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Color32, Rect, Response, RichText, Shape, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::app::EpanetApp;
use crate::basemap::Geo;

/// The kind of service a layer comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Service {
    #[default]
    Wms,
    ArcGis,
}

impl Service {
    fn label(self) -> &'static str {
        match self {
            Service::Wms => "WMS",
            Service::ArcGis => "ArcGIS REST map service",
        }
    }
}

/// A layer the user added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapLayer {
    pub name: String,
    pub service: Service,
    /// Where pictures are asked for: the WMS GetMap address or the map
    /// service's address.
    pub url: String,
    /// WMS layer names, or ArcGIS layer ids (all when empty).
    pub layers: Vec<String>,
    /// WMS version.
    pub version: String,
    /// The coordinate system pictures are asked in.
    pub crs: String,
    pub visible: bool,
    pub opacity: f32,
    /// The group it was added in, shown above it in the Map Layers panel.
    pub group: String,
}

impl Default for MapLayer {
    fn default() -> Self {
        MapLayer {
            name: String::new(),
            service: Service::Wms,
            url: String::new(),
            layers: Vec::new(),
            version: "1.3.0".into(),
            crs: WEB_MERCATOR.into(),
            visible: true,
            opacity: 1.0,
            group: String::new(),
        }
    }
}

const WEB_MERCATOR: &str = "EPSG:3857";
/// The coordinate systems pictures can be asked in, best first.
const CRS_PREFERRED: [&str; 6] = [
    "EPSG:3857",
    "EPSG:900913",
    "EPSG:102100",
    "EPSG:102113",
    "CRS:84",
    "EPSG:4326",
];
/// How long the view must stay still before a new picture is asked for (s).
const SETTLE: f64 = 0.35;
/// The largest picture asked for, on either side (pixels).
const MAX_SIDE: f64 = 2048.0;
/// The URLs remembered for the Add Layer window.
const MAX_URLS: usize = 10;

fn mercator(crs: &str) -> bool {
    CRS_PREFERRED[..4]
        .iter()
        .any(|c| c.eq_ignore_ascii_case(crs))
}

impl MapLayer {
    /// What tells this layer's pictures apart from another's.
    fn key(&self) -> String {
        format!(
            "{:?}|{}|{}|{}",
            self.service,
            self.url,
            self.layers.join(","),
            self.crs
        )
    }

    /// The address of a `w` x `h` picture of `b` (in the layer's coordinate
    /// system: x min, y min, x max, y max).
    fn image_url(&self, b: [f64; 4], w: u32, h: u32) -> String {
        match self.service {
            Service::ArcGis => {
                let mut u = format!(
                    "{}/export?bbox={},{},{},{}&bboxSR=3857&imageSR=3857&size={w},{h}\
                     &format=png32&transparent=true&dpi=96&f=image",
                    self.url.trim_end_matches('/'),
                    b[0],
                    b[1],
                    b[2],
                    b[3]
                );
                if !self.layers.is_empty() {
                    u += &format!("&layers=show:{}", self.layers.join(","));
                }
                u
            }
            Service::Wms => {
                let v13 = self.version.starts_with("1.3");
                // WMS 1.3.0 puts latitude first in EPSG:4326
                let bbox = if v13 && self.crs.eq_ignore_ascii_case("EPSG:4326") {
                    [b[1], b[0], b[3], b[2]]
                } else {
                    b
                };
                format!(
                    "{}SERVICE=WMS&VERSION={}&REQUEST=GetMap&LAYERS={}&STYLES=\
                     &FORMAT=image%2Fpng&TRANSPARENT=TRUE&WIDTH={w}&HEIGHT={h}&{}={}\
                     &BBOX={},{},{},{}",
                    with_query(&self.url),
                    self.version,
                    encode(&self.layers.join(",")),
                    if v13 { "CRS" } else { "SRS" },
                    encode(&self.crs),
                    bbox[0],
                    bbox[1],
                    bbox[2],
                    bbox[3]
                )
            }
        }
    }
}

/// `url` ready for more query parameters.
fn with_query(url: &str) -> String {
    if !url.contains('?') {
        format!("{url}?")
    } else if url.ends_with('?') || url.ends_with('&') {
        url.to_string()
    } else {
        format!("{url}&")
    }
}

/// Percent-encodes a query value.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => {
                out.push(b as char)
            }
            _ => out += &format!("%{b:02X}"),
        }
    }
    out
}

/// `url` without the query parameters `names` (in any case).
fn without_params(url: &str, names: &[&str]) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|p| {
            let key = p.split('=').next().unwrap_or("");
            !p.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(key))
        })
        .collect();
    if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

/// The service an address points at.
fn guess_service(url: &str) -> Option<Service> {
    let u = url.to_ascii_lowercase();
    if u.contains("/rest/services/") {
        Some(Service::ArcGis)
    } else if u.contains("service=wms") || u.contains("wmsserver") || u.contains("/wms") {
        Some(Service::Wms)
    } else {
        None
    }
}

// ----- capabilities -----------------------------------------------------------

/// What a service offers.
#[derive(Clone, Debug, Default, PartialEq)]
struct Caps {
    title: String,
    service: Service,
    /// Where pictures are asked for.
    url: String,
    version: String,
    crs: String,
    layers: Vec<CapLayer>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CapLayer {
    /// The WMS name or the ArcGIS id; empty for a WMS layer that only groups others.
    name: String,
    title: String,
    depth: usize,
    /// Shown by default.
    default: bool,
}

/// An ArcGIS map service's address, without what follows `MapServer`.
fn arcgis_base(url: &str) -> String {
    let u = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    u.strip_suffix("/export").unwrap_or(u).to_string()
}

/// Where a service's description is read from.
fn caps_url(service: Service, url: &str) -> String {
    match service {
        Service::ArcGis => format!("{}?f=json", arcgis_base(url)),
        Service::Wms => format!(
            "{}SERVICE=WMS&REQUEST=GetCapabilities",
            with_query(&without_params(url, &["service", "request", "version"]))
        ),
    }
}

fn parse_arcgis(json: &str, url: &str) -> Result<Caps, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "Not an ArcGIS map service".to_string())?;
    if let Some(e) = v.get("error") {
        return Err(e
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("The service answered with an error")
            .to_string());
    }
    let base = arcgis_base(url);
    let list = v
        .get("layers")
        .and_then(|l| l.as_array())
        .ok_or("Not an ArcGIS map service (no layers)")?;
    let parent: HashMap<i64, i64> = list
        .iter()
        .filter_map(|l| Some((l.get("id")?.as_i64()?, l.get("parentLayerId")?.as_i64()?)))
        .collect();
    let depth = |mut id: i64| {
        let mut d = 0;
        while let Some(&p) = parent.get(&id) {
            if p < 0 || d > 20 {
                break;
            }
            d += 1;
            id = p;
        }
        d
    };
    let layers = list
        .iter()
        .filter_map(|l| {
            let id = l.get("id")?.as_i64()?;
            Some(CapLayer {
                name: id.to_string(),
                title: l.get("name")?.as_str()?.to_string(),
                depth: depth(id),
                default: l
                    .get("defaultVisibility")
                    .and_then(|d| d.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect();
    let title = v
        .get("mapName")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty() && *t != "Layers")
        .map(str::to_string)
        .unwrap_or_else(|| {
            base.trim_end_matches("/MapServer")
                .rsplit('/')
                .next()
                .unwrap_or("Map service")
                .to_string()
        });
    Ok(Caps {
        title,
        service: Service::ArcGis,
        url: base,
        version: String::new(),
        crs: WEB_MERCATOR.into(),
        layers,
    })
}

fn parse_wms(xml: &str, url: &str) -> Result<Caps, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|_| "Not a WMS (no capabilities)")?;
    let root = doc.root_element();
    if !root.tag_name().name().contains("Capabilities") {
        let msg = root
            .descendants()
            .find(|n| n.tag_name().name() == "ServiceException")
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string());
        return Err(msg.unwrap_or_else(|| "Not a WMS (no capabilities)".into()));
    }
    let version = root.attribute("version").unwrap_or("1.3.0").to_string();
    let child = |n: roxmltree::Node, name: &str| {
        n.children()
            .find(|c| c.tag_name().name() == name)
            .and_then(|c| c.text())
            .map(|t| t.trim().to_string())
    };
    let title = root
        .children()
        .find(|n| n.tag_name().name() == "Service")
        .and_then(|s| child(s, "Title"))
        .unwrap_or_else(|| "WMS".into());
    // where GetMap is to be asked, if the service says
    let get_map = root
        .descendants()
        .find(|n| n.tag_name().name() == "GetMap")
        .and_then(|g| {
            g.descendants()
                .filter(|n| n.tag_name().name() == "Get")
                .flat_map(|n| n.descendants())
                .find(|n| n.tag_name().name() == "OnlineResource")
        })
        .and_then(|r| {
            r.attributes()
                .find(|a| a.name() == "href")
                .map(|a| a.value().to_string())
        })
        .filter(|u| u.starts_with("http"));
    let capability = root
        .children()
        .find(|n| n.tag_name().name() == "Capability")
        .ok_or("Not a WMS (no Capability)")?;
    let mut layers = Vec::new();
    let mut crs: Vec<String> = Vec::new();
    fn walk(n: roxmltree::Node, depth: usize, layers: &mut Vec<CapLayer>, crs: &mut Vec<String>) {
        for c in n.children().filter(|c| c.tag_name().name() == "Layer") {
            for s in c
                .children()
                .filter(|s| matches!(s.tag_name().name(), "CRS" | "SRS"))
            {
                for code in s.text().unwrap_or("").split_whitespace() {
                    if !crs.iter().any(|x| x.eq_ignore_ascii_case(code)) {
                        crs.push(code.to_string());
                    }
                }
            }
            let text = |name: &str| {
                c.children()
                    .find(|x| x.tag_name().name() == name)
                    .and_then(|x| x.text())
                    .map(|t| t.trim().to_string())
                    .unwrap_or_default()
            };
            let name = text("Name");
            let title = text("Title");
            layers.push(CapLayer {
                title: if title.is_empty() {
                    name.clone()
                } else {
                    title
                },
                name,
                depth,
                default: false,
            });
            walk(c, depth + 1, layers, crs);
        }
    }
    walk(capability, 0, &mut layers, &mut crs);
    if layers.iter().all(|l| l.name.is_empty()) {
        return Err("The WMS offers no layers".into());
    }
    let chosen = CRS_PREFERRED
        .iter()
        .find(|p| crs.iter().any(|c| c.eq_ignore_ascii_case(p)))
        .ok_or(
            "The WMS offers neither Web Mercator (EPSG:3857) nor longitude/latitude (EPSG:4326)",
        )?;
    let base = without_params(url, &["service", "request", "version"]);
    Ok(Caps {
        title,
        service: Service::Wms,
        url: get_map.unwrap_or(base),
        version,
        crs: chosen.to_string(),
        layers,
    })
}

// ----- state ----------------------------------------------------------------

/// What a service answered, once it has.
type Reply = Arc<Mutex<Option<Result<Caps, String>>>>;

type Delivered = Arc<Mutex<Vec<(String, u64, Result<egui::ColorImage, String>)>>>;

/// A layer's pictures.
#[derive(Default)]
struct View {
    /// The picture in view, what it covers and its address.
    shown: Option<(egui::TextureHandle, [f64; 4], String)>,
    /// The picture asked for: its request, what it covers and its address.
    loading: Option<(u64, [f64; 4], String)>,
    /// The address the view called for, and since when.
    wanted: Option<(String, f64)>,
    error: Option<String>,
}

/// The Add Layer window.
#[derive(Default)]
struct Adding {
    service: Service,
    url: String,
    caps: Option<Caps>,
    checked: Vec<bool>,
    name: String,
    error: Option<String>,
    reply: Option<Reply>,
}

#[derive(Default)]
pub struct LayersState {
    views: HashMap<String, View>,
    delivered: Delivered,
    next: u64,
    adding: Adding,
    /// The name being edited in the Map Layers panel, and its new text.
    renaming: Option<(Rename, String)>,
}

// ----- drawing --------------------------------------------------------------

/// The part of the layer's coordinate system in view, and the picture size
/// that covers it pixel for pixel.
fn wanted_area(
    geo: &Geo,
    view: &crate::map::MapView,
    rect: Rect,
    ppp: f32,
    merc: bool,
) -> Option<([f64; 4], u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for i in 0..=6 {
        for j in 0..=6 {
            let p = rect.min
                + vec2(
                    rect.width() * i as f32 / 6.0,
                    rect.height() * j as f32 / 6.0,
                );
            let Some([lon, lat]) = geo.to_lonlat(view.to_world(rect, p)) else {
                continue;
            };
            let [x, y] = if merc {
                Geo::WebMercator.lonlat_to([lon, lat.clamp(-85.0, 85.0)])?
            } else {
                [lon, lat]
            };
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let mut w = (rect.width() * ppp) as f64;
    let mut h = (rect.height() * ppp) as f64;
    // square pixels: the area grows to the picture's proportions
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (mut bw, mut bh) = (x1 - x0, y1 - y0);
    if bw / bh > w / h {
        bh = bw * h / w;
    } else {
        bw = bh * w / h;
    }
    let s = (MAX_SIDE / w.max(h)).min(1.0);
    w = (w * s).round().max(16.0);
    h = (h * s).round().max(16.0);
    Some((
        [cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0],
        w as u32,
        h as u32,
    ))
}

/// A picture of `b` as a grid of quads whose corners go through the
/// project's coordinate transformation.
fn picture_mesh(
    geo: &Geo,
    view: &crate::map::MapView,
    rect: Rect,
    b: [f64; 4],
    merc: bool,
    tex: egui::TextureId,
    color: Color32,
) -> Option<egui::Mesh> {
    const N: u32 = 12;
    let mut mesh = egui::Mesh::with_texture(tex);
    for j in 0..=N {
        for i in 0..=N {
            let (fx, fy) = (i as f64 / N as f64, j as f64 / N as f64);
            let p = [b[0] + fx * (b[2] - b[0]), b[3] - fy * (b[3] - b[1])];
            let lonlat = if merc {
                Geo::WebMercator.to_lonlat(p)?
            } else {
                p
            };
            let world = geo.lonlat_to(lonlat)?;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: view.to_screen(rect, world),
                uv: pos2(fx as f32, fy as f32),
                color,
            });
        }
    }
    for j in 0..N {
        for i in 0..N {
            let a = j * (N + 1) + i;
            mesh.add_triangle(a, a + 1, a + N + 1);
            mesh.add_triangle(a + 1, a + N + 2, a + N + 1);
        }
    }
    Some(mesh)
}

/// Takes in the pictures that arrived.
fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let st = &mut app.layers;
    let arrived: Vec<_> = st
        .delivered
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    for (key, id, res) in arrived {
        let Some(v) = st.views.get_mut(&key) else {
            continue;
        };
        let Some((_, b, url)) = v.loading.clone().filter(|l| l.0 == id) else {
            continue; // a picture no longer wanted
        };
        v.loading = None;
        match res {
            Ok(img) => {
                let tex =
                    ctx.load_texture(format!("layer {key}"), img, egui::TextureOptions::LINEAR);
                v.shown = Some((tex, b, url));
                v.error = None;
            }
            Err(e) => v.error = Some(e),
        }
    }
}

fn request(st: &mut LayersState, ctx: &egui::Context, key: String, url: String, b: [f64; 4]) {
    st.next += 1;
    let id = st.next;
    st.views.entry(key.clone()).or_default().loading = Some((id, b, url.clone()));
    let delivered = st.delivered.clone();
    let ctx = ctx.clone();
    ehttp::fetch(ehttp::Request::get(url), move |res| {
        let img = match res {
            Ok(r) if r.ok => image::load_from_memory(&r.bytes)
                .map(|i| {
                    let rgba = i.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
                })
                .map_err(|_| {
                    // services answer errors as text or XML
                    let text = String::from_utf8_lossy(&r.bytes);
                    let text = text.trim();
                    if text.is_empty() {
                        "The service sent no picture".to_string()
                    } else {
                        text.chars().take(160).collect()
                    }
                }),
            Ok(r) => Err(format!("{} {}", r.status, r.status_text)),
            Err(e) => Err(e),
        };
        if let Ok(mut q) = delivered.lock() {
            q.push((key, id, img));
        }
        ctx.request_repaint();
    });
}

/// Draws the visible layers over `rect`, bottom one first, asking for new
/// pictures once the view has settled.
pub fn draw(app: &mut EpanetApp, ui: &Ui, painter: &egui::Painter, rect: Rect) {
    let ctx = ui.ctx().clone();
    poll(app, &ctx);
    if !app.prefs.map_layers.iter().any(|l| l.visible) || crate::basemap::geo(app).is_err() {
        return;
    }
    let mut st = std::mem::take(&mut app.layers);
    let Ok(geo) = crate::basemap::geo_ref(app) else {
        app.layers = st;
        return;
    };
    let now = ui.input(|i| i.time);
    let ppp = ctx.pixels_per_point();
    for layer in app.prefs.map_layers.iter().filter(|l| l.visible) {
        let key = layer.key();
        let merc = mercator(&layer.crs);
        if let Some((b, w, h)) = wanted_area(geo, &app.view, rect, ppp, merc) {
            let url = layer.image_url(b, w, h);
            let v = st.views.entry(key.clone()).or_default();
            let have = v.shown.as_ref().is_some_and(|s| s.2 == url)
                || v.loading.as_ref().is_some_and(|l| l.2 == url);
            if !have {
                match &v.wanted {
                    Some((u, since)) if *u == url && now - since >= SETTLE => {
                        v.wanted = None;
                        request(&mut st, &ctx, key.clone(), url, b);
                    }
                    Some((u, _)) if *u == url => {}
                    _ => v.wanted = Some((url, now)),
                }
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE));
            }
        }
        if let Some((tex, b, _)) = st.views.get(&key).and_then(|v| v.shown.as_ref()) {
            let color = Color32::WHITE.gamma_multiply(layer.opacity.clamp(0.0, 1.0));
            if let Some(mesh) = picture_mesh(geo, &app.view, rect, *b, merc, tex.id(), color) {
                painter.add(Shape::mesh(mesh));
            }
        }
    }
    app.layers = st;
}

// ----- the panel ------------------------------------------------------------

/// The Map Layers panel (see [`crate::dock`]).
pub(crate) fn pane(app: &mut EpanetApp, ui: &mut Ui) {
    let red = Color32::from_rgb(0xc0, 0x39, 0x2b);
    egui::ScrollArea::vertical()
        .id_salt("map_layers")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let layers = &app.prefs.map_layers;
                    if layers.is_empty() {
                        ui.label(
                            RichText::new(crate::i18n::tr(
                                "No layers yet: add one from a WMS or an ArcGIS REST map service (Map > Layers > Add Layer).",
                            ))
                            .weak(),
                        );
                        return;
                    }
                    if crate::basemap::crs_def(app).is_none() {
                        ui.label(
                            RichText::new(crate::i18n::tr(
                                "Set the project's coordinate reference system (CRS in the status bar) to show these layers.",
                            ))
                            .color(red),
                        );
                        ui.add_space(4.0);
                    }
                    list(app, ui, red);
                });
        });
}

/// What a click in the list does.
enum Change {
    Remove(usize),
    /// Up (1) or down (-1) one place.
    Move(usize, i32),
    RemoveGroup(String),
}

/// The layers, the top one first as they are drawn, under the names of
/// their groups.
fn list(app: &mut EpanetApp, ui: &mut Ui, red: Color32) {
    let n = app.prefs.map_layers.len();
    let mut change = None;
    let mut renaming = std::mem::take(&mut app.layers.renaming);
    let mut renamed = None;
    let mut k = n;
    while k > 0 {
        // the run of layers of one group, from the top
        let group = app.prefs.map_layers[k - 1].group.clone();
        let mut first = k - 1;
        while first > 0 && app.prefs.map_layers[first - 1].group == group {
            first -= 1;
        }
        let members = first..k;
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !group.is_empty() {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if glyph_button(ui, true, Glyph::Remove, "Remove the group").clicked() {
                                change = Some(Change::RemoveGroup(group.clone()));
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let layers = &mut app.prefs.map_layers[members.clone()];
                                    let mut all = layers.iter().all(|l| l.visible);
                                    if ui
                                        .add(crate::widgets::Checkbox::new(&mut all, ""))
                                        .changed()
                                    {
                                        for l in layers.iter_mut() {
                                            l.visible = all;
                                        }
                                    }
                                    if let Some(done) = name_field(
                                        ui,
                                        &mut renaming,
                                        Rename::Group(group.clone()),
                                        &group,
                                        RichText::new(&group).strong(),
                                        &group,
                                    ) {
                                        renamed = Some(done);
                                    }
                                },
                            );
                        });
                    });
                }
                for j in members.clone().rev() {
                    let key = app.prefs.map_layers[j].key();
                    let view = app.layers.views.get(&key);
                    let loading = view.is_some_and(|v| v.loading.is_some());
                    let error = view.and_then(|v| v.error.clone());
                    let l = &mut app.prefs.map_layers[j];
                    let indent = if group.is_empty() { 0.0 } else { 16.0 };
                    ui.horizontal(|ui| {
                        ui.add_space(indent);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if glyph_button(ui, true, Glyph::Remove, "Remove").clicked() {
                                change = Some(Change::Remove(j));
                            }
                            if glyph_button(ui, j > 0, Glyph::Down, "Move down").clicked() {
                                change = Some(Change::Move(j, -1));
                            }
                            if glyph_button(ui, j + 1 < n, Glyph::Up, "Move up").clicked() {
                                change = Some(Change::Move(j, 1));
                            }
                            if loading {
                                ui.spinner();
                            }
                            // the name in the room the buttons leave
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(crate::widgets::Checkbox::new(&mut l.visible, ""));
                                    let tip =
                                        format!("{}\n{}\n{}", l.name, l.service.label(), l.url);
                                    if let Some(done) = name_field(
                                        ui,
                                        &mut renaming,
                                        Rename::Layer(j),
                                        &l.name,
                                        RichText::new(&l.name),
                                        &tip,
                                    ) {
                                        renamed = Some(done);
                                    }
                                },
                            );
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(indent);
                        ui.label(crate::i18n::tr("Opacity"));
                        let mut o = l.opacity as f64;
                        crate::widgets::slider(ui, true, &mut o, 0.0..=1.0, false, None);
                        l.opacity = o as f32;
                    });
                    if let Some(e) = error {
                        ui.horizontal(|ui| {
                            ui.add_space(indent);
                            ui.add(egui::Label::new(RichText::new(e).color(red).small()).wrap());
                        });
                    }
                }
            });
        ui.add_space(4.0);
        k = first;
    }
    app.layers.renaming = renaming;
    let layers = &mut app.prefs.map_layers;
    match renamed {
        Some((Rename::Layer(j), name)) => {
            if let Some(l) = layers.get_mut(j) {
                l.name = name;
            }
        }
        Some((Rename::Group(old), name)) => {
            for l in layers.iter_mut().filter(|l| l.group == old) {
                l.group = name.clone();
            }
        }
        None => {}
    }
    let gone: Vec<MapLayer> = match change {
        Some(Change::Remove(j)) => vec![layers.remove(j)],
        Some(Change::Move(j, d)) => {
            layers.swap(j, (j as i32 + d) as usize);
            Vec::new()
        }
        Some(Change::RemoveGroup(g)) => {
            let (out, keep) = std::mem::take(layers)
                .into_iter()
                .partition(|l| l.group == g);
            *layers = keep;
            out
        }
        None => Vec::new(),
    };
    for l in gone {
        app.layers.views.remove(&l.key());
    }
}

/// What is being renamed in the Map Layers panel.
#[derive(Clone, Debug, PartialEq)]
enum Rename {
    Layer(usize),
    Group(String),
}

/// A layer's or a group's name: double-click it (or right-click > Rename) to
/// rename it, Enter to keep the new name, Escape to leave it. Returns what
/// was renamed and its new name.
fn name_field(
    ui: &mut Ui,
    renaming: &mut Option<(Rename, String)>,
    what: Rename,
    name: &str,
    text: RichText,
    tip: &str,
) -> Option<(Rename, String)> {
    match renaming {
        Some((r, buf)) if *r == what => {
            let id = egui::Id::new("map_layer_rename");
            let resp = ui.add(
                egui::TextEdit::singleline(buf)
                    .id(id)
                    .desired_width(ui.available_width()),
            );
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                *renaming = None;
                return None;
            }
            if resp.lost_focus() {
                let (r, new) = renaming.take()?;
                let new = new.trim().to_string();
                return (!new.is_empty() && new != name).then_some((r, new));
            }
            None
        }
        _ => {
            let resp = ui
                .add(
                    egui::Label::new(text)
                        .truncate()
                        .sense(egui::Sense::click()),
                )
                .on_hover_text(format!(
                    "{tip}\n\n{}",
                    crate::i18n::tr("Double-click to rename")
                ));
            let mut start = resp.double_clicked();
            resp.context_menu(|ui| {
                if ui.button(crate::i18n::tr("Rename")).clicked() {
                    start = true;
                    ui.close();
                }
            });
            if start {
                *renaming = Some((what, name.to_string()));
            }
            None
        }
    }
}

#[derive(Clone, Copy)]
enum Glyph {
    Up,
    Down,
    Remove,
}

/// A small button with a drawn arrow or cross (the interface font has no
/// such symbols).
fn glyph_button(ui: &mut Ui, enabled: bool, glyph: Glyph, tip: &str) -> Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), sense);
    let v = ui.visuals();
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, 3.0, v.widgets.hovered.weak_bg_fill);
    }
    let color = if enabled {
        v.text_color()
    } else {
        v.weak_text_color().gamma_multiply(0.5)
    };
    let c = rect.center();
    let d = 4.5;
    match glyph {
        Glyph::Up | Glyph::Down => {
            let s = if matches!(glyph, Glyph::Up) {
                -1.0
            } else {
                1.0
            };
            ui.painter().add(Shape::convex_polygon(
                vec![
                    c + vec2(-d, -s * d * 0.6),
                    c + vec2(d, -s * d * 0.6),
                    c + vec2(0.0, s * d * 0.8),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        Glyph::Remove => {
            let s = egui::Stroke::new(1.6, color);
            ui.painter()
                .line_segment([c - vec2(d, d), c + vec2(d, d)], s);
            ui.painter()
                .line_segment([c + vec2(-d, d), c + vec2(d, -d)], s);
        }
    }
    resp.on_hover_text(crate::i18n::tr(tip))
}

// ----- adding a layer ---------------------------------------------------------

/// Opens the Add Layer window (and the panel).
pub fn open_add(app: &mut EpanetApp) {
    app.prefs.hide_layers = false;
    app.prefs.docks.activate(crate::dock::Pane::Layers);
    let last = app.prefs.layer_urls.first().cloned().unwrap_or_default();
    let st = &mut app.layers.adding;
    if st.url.is_empty() {
        st.service = guess_service(&last).unwrap_or_default();
        st.url = last;
    }
    st.error = None;
    app.dialog = crate::app::Dialog::AddLayer;
}

/// Reads what the service at the address typed offers.
fn connect(app: &mut EpanetApp, ctx: &egui::Context) {
    let st = &mut app.layers.adding;
    let url = st.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        st.error = Some("Enter the service's address (http:// or https://)".into());
        return;
    }
    if let Some(s) = guess_service(&url) {
        st.service = s;
    }
    let service = st.service;
    st.caps = None;
    st.error = None;
    let reply = Arc::new(Mutex::new(None));
    st.reply = Some(reply.clone());
    let ctx = ctx.clone();
    let source = url.clone();
    ehttp::fetch(ehttp::Request::get(caps_url(service, &url)), move |res| {
        let caps = match res {
            Ok(r) if r.ok => {
                let text = crate::io::decode_text(&r.bytes);
                match service {
                    Service::ArcGis => parse_arcgis(&text, &source),
                    Service::Wms => parse_wms(&text, &source),
                }
            }
            Ok(r) => Err(format!("{} {}", r.status, r.status_text)),
            Err(e) => Err(format!("Could not reach the service: {e}")),
        };
        if let Ok(mut r) = reply.lock() {
            *r = Some(caps);
        }
        ctx.request_repaint();
    });
    let urls = &mut app.prefs.layer_urls;
    urls.retain(|u| *u != url);
    urls.insert(0, url);
    urls.truncate(MAX_URLS);
}

/// Takes in what the service answered.
/// A name for the group of a service's layers: an ArcGIS service's name in
/// its address ("…/National/CadastralMap_GR/MapServer": "Cadastral Map GR"),
/// a WMS's title, or (a WMS called just "WMS") a name from its address.
fn suggested_group(c: &Caps) -> String {
    let path = c.url.split('?').next().unwrap_or(&c.url);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let from_url = || match c.service {
        Service::ArcGis => segments
            .iter()
            .position(|s| s.eq_ignore_ascii_case("MapServer"))
            .and_then(|i| i.checked_sub(1))
            .map(|i| segments[i]),
        Service::Wms => segments
            .iter()
            .rev()
            .find(|s| {
                let s = s.to_ascii_lowercase();
                !(s.contains("wms") || s == "ows" || s.ends_with(".php") || s.contains("cgi"))
            })
            .copied()
            .filter(|s| !s.contains(':') && !s.contains('.')),
    };
    let title = c.title.trim();
    let generic = title.is_empty()
        || ["wms", "web map service", "ows", "layers"]
            .iter()
            .any(|g| title.eq_ignore_ascii_case(g));
    match c.service {
        Service::Wms if !generic => title.to_string(),
        _ => from_url()
            .map(pretty)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if generic {
                    host(&c.url)
                } else {
                    title.to_string()
                }
            }),
    }
}

/// "CadastralMap_GR" as "Cadastral Map GR".
fn pretty(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for ch in s.chars() {
        let ch = if ch == '_' || ch == '-' { ' ' } else { ch };
        if ch.is_uppercase() && prev.is_some_and(|p| p.is_lowercase()) {
            out.push(' ');
        }
        out.push(ch);
        prev = Some(ch);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The host of an address.
fn host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

fn take_caps(st: &mut Adding) {
    let Some(caps) = st.reply.as_ref().and_then(|r| r.lock().ok()?.take()) else {
        return;
    };
    st.reply = None;
    match caps {
        Ok(c) => {
            st.checked = c.layers.iter().map(|l| l.default).collect();
            st.name = suggested_group(&c);
            st.caps = Some(c);
        }
        Err(e) => st.error = Some(e),
    }
}

/// The layers the window describes, one for each layer chosen (of a group
/// chosen with some of its layers, those layers), all in one group; none if
/// nothing can be added.
fn chosen(st: &Adding) -> Vec<MapLayer> {
    let Some(caps) = st.caps.as_ref() else {
        return Vec::new();
    };
    let group = if st.name.trim().is_empty() {
        suggested_group(caps)
    } else {
        st.name.trim().to_string()
    };
    let on = |k: usize| st.checked.get(k).copied().unwrap_or(false);
    let mut out = Vec::new();
    for (k, l) in caps.layers.iter().enumerate() {
        if !on(k) || l.name.is_empty() {
            continue;
        }
        let inner_chosen = descendants(&caps.layers, k).any(on);
        if inner_chosen {
            continue;
        }
        out.push(MapLayer {
            name: l.title.clone(),
            service: caps.service,
            url: caps.url.clone(),
            layers: vec![l.name.clone()],
            version: caps.version.clone(),
            crs: caps.crs.clone(),
            group: group.clone(),
            ..Default::default()
        });
    }
    // an ArcGIS service with nothing chosen: all it shows by itself
    if out.is_empty() && caps.service == Service::ArcGis && !st.checked.iter().any(|c| *c) {
        out.push(MapLayer {
            name: group.clone(),
            service: caps.service,
            url: caps.url.clone(),
            crs: caps.crs.clone(),
            group,
            ..Default::default()
        });
    }
    out
}

/// The positions of the layers inside the `k`th of `layers` (listed after it,
/// deeper).
fn descendants(layers: &[CapLayer], k: usize) -> impl Iterator<Item = usize> + '_ {
    let depth = layers[k].depth;
    (k + 1..layers.len()).take_while(move |&j| layers[j].depth > depth)
}

/// The Add Layer window: a service's address, its layers to choose from.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    take_caps(&mut app.layers.adding);
    ui.set_min_width(560.0);
    let recent = app.prefs.layer_urls.clone();
    let mut go = false;
    {
        let st = &mut app.layers.adding;
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Service"));
            ui.add_space(8.0);
            for s in [Service::Wms, Service::ArcGis] {
                crate::widgets::radio_value(ui, &mut st.service, s, s.label());
                ui.add_space(12.0);
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("URL");
            let r = ui.add(
                egui::TextEdit::singleline(&mut st.url)
                    .desired_width(400.0)
                    .hint_text("https://…/MapServer  or  https://…/wms"),
            );
            if r.changed()
                && let Some(s) = guess_service(&st.url)
            {
                st.service = s;
            }
            go = ui.button(crate::i18n::tr("Connect")).clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        });
        if !recent.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(crate::i18n::tr("Used before")).weak());
                egui::ComboBox::from_id_salt("layer_urls")
                    .selected_text("…")
                    .width(400.0)
                    .show_ui(ui, |ui| {
                        for u in &recent {
                            if ui.selectable_label(false, u).clicked() {
                                st.url = u.clone();
                                if let Some(s) = guess_service(u) {
                                    st.service = s;
                                }
                                go = true;
                            }
                        }
                    });
            });
        }
    }
    if go {
        connect(app, &ctx);
    }
    let st = &mut app.layers.adding;
    ui.add_space(8.0);
    if st.reply.is_some() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(crate::i18n::tr("Connecting…"));
        });
    }
    if let Some(e) = &st.error {
        ui.label(RichText::new(e).color(Color32::from_rgb(0xc0, 0x39, 0x2b)));
    }
    if let Some(caps) = &st.caps {
        ui.label(RichText::new(&caps.title).strong());
        ui.label(
            RichText::new(match caps.service {
                Service::Wms => format!("WMS {} · {}", caps.version, caps.crs),
                Service::ArcGis => crate::i18n::tr("ArcGIS REST map service").to_string(),
            })
            .weak(),
        );
        ui.add_space(4.0);
        egui::Frame::NONE
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(6)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("layer_caps")
                    .max_height(260.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut toggled = None;
                        for (k, l) in caps.layers.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_space(l.depth as f32 * 16.0);
                                if l.name.is_empty() {
                                    ui.label(RichText::new(&l.title).strong());
                                } else if crate::widgets::checkbox(ui, &mut st.checked[k], &l.title)
                                    .changed()
                                {
                                    toggled = Some(k);
                                }
                            });
                        }
                        // a group checked or not: its layers with it
                        if let Some(k) = toggled {
                            let on = st.checked[k];
                            for j in descendants(&caps.layers, k).collect::<Vec<_>>() {
                                st.checked[j] = on;
                            }
                        }
                    });
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Group"));
            ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(400.0));
        });
    }
    ui.add_space(10.0);
    let layers = chosen(st);
    let mut close = false;
    match crate::dialogs::buttons(ui, &["Add", "Cancel"]) {
        Some(0) => match layers.first().map(|l| l.group.clone()) {
            Some(group) => {
                app.status = format!("Added {} map layers in {group}", layers.len());
                app.prefs.map_layers.extend(layers);
                app.prefs.hide_layers = false;
                app.prefs.docks.activate(crate::dock::Pane::Layers);
                app.layers.adding = Adding::default();
                close = true;
            }
            None => {
                app.layers.adding.error = Some(if app.layers.adding.caps.is_none() {
                    "Connect to a service first".into()
                } else {
                    "Choose at least one layer".into()
                })
            }
        },
        Some(_) => close = true,
        None => {}
    }
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCGIS: &str = r#"{"currentVersion":10.91,"mapName":"ΤΕΜΑΧΙΑ","layers":[
        {"id":0,"name":"ΤΕΜΑΧΙΑ","parentLayerId":-1,"defaultVisibility":true},
        {"id":1,"name":"ΦΥΛΛΑ_ΣΧΕΔΙΑ","parentLayerId":-1,"defaultVisibility":false,"subLayerIds":[2]},
        {"id":2,"name":"Cassini_1_5000","parentLayerId":1,"defaultVisibility":false}],
        "spatialReference":{"wkid":102100,"latestWkid":3857}}"#;

    const WMS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<WMS_Capabilities version="1.3.0" xmlns="http://www.opengis.net/wms"
  xmlns:xlink="http://www.w3.org/1999/xlink">
  <Service><Name>WMS</Name><Title>Cadastral Map</Title></Service>
  <Capability>
    <Request><GetMap><Format>image/png</Format><DCPType><HTTP><Get>
      <OnlineResource xlink:type="simple" xlink:href="https://maps.example.org/wms?map=x&amp;"/>
    </Get></HTTP></DCPType></GetMap></Request>
    <Layer><Title>Everything</Title><CRS>EPSG:4326</CRS><CRS>EPSG:3857</CRS>
      <Layer><Name>parcels</Name><Title>Parcels</Title></Layer>
      <Layer><Name>roads</Name><Title>Roads</Title></Layer>
    </Layer>
  </Capability>
</WMS_Capabilities>"#;

    #[test]
    fn reads_an_arcgis_map_service() {
        let url =
            "https://e.example/arcgis/rest/services/National/CadastralMap_GR/MapServer?f=pjson";
        assert_eq!(guess_service(url), Some(Service::ArcGis));
        assert_eq!(
            caps_url(Service::ArcGis, url),
            "https://e.example/arcgis/rest/services/National/CadastralMap_GR/MapServer?f=json"
        );
        let c = parse_arcgis(ARCGIS, url).unwrap();
        assert_eq!(c.title, "ΤΕΜΑΧΙΑ");
        assert_eq!(c.layers.len(), 3);
        assert_eq!((c.layers[2].depth, c.layers[0].default), (1, true));
        let mut st = Adding {
            checked: c.layers.iter().map(|l| l.default).collect(),
            name: c.title.clone(),
            caps: Some(c),
            ..Default::default()
        };
        // a group chosen with one of its layers, and a layer by itself: each
        // its own layer, in the group named
        st.checked = vec![true, true, true];
        let all = chosen(&st);
        let names: Vec<_> = all
            .iter()
            .map(|l| (l.name.as_str(), l.layers.clone()))
            .collect();
        assert_eq!(
            names,
            [
                ("ΤΕΜΑΧΙΑ", vec!["0".to_string()]),
                ("Cassini_1_5000", vec!["2".to_string()])
            ]
        );
        assert!(all.iter().all(|l| l.group == "ΤΕΜΑΧΙΑ"));
        // the group alone: the group, showing all its layers
        st.checked = vec![false, true, false];
        assert_eq!(chosen(&st)[0].layers, ["1"]);
        st.checked = vec![true, false, false];
        let l = chosen(&st).remove(0);
        assert_eq!(l.layers, ["0"]);
        let u = l.image_url([1.0, 2.0, 3.0, 4.0], 800, 600);
        assert!(
            u.starts_with(
                "https://e.example/arcgis/rest/services/National/CadastralMap_GR/MapServer/export?bbox=1,2,3,4&bboxSR=3857&imageSR=3857&size=800,600"
            ),
            "{u}"
        );
        assert!(u.ends_with("&layers=show:0"), "{u}");
        assert!(
            parse_arcgis(r#"{"error":{"code":499,"message":"Token Required"}}"#, url)
                .unwrap_err()
                .contains("Token")
        );
    }

    #[test]
    fn reads_a_wms() {
        let url = "https://maps.example.org/wms?map=x&SERVICE=WMS&REQUEST=GetCapabilities";
        assert_eq!(guess_service(url), Some(Service::Wms));
        assert_eq!(
            caps_url(Service::Wms, url),
            "https://maps.example.org/wms?map=x&SERVICE=WMS&REQUEST=GetCapabilities"
        );
        let c = parse_wms(WMS, url).unwrap();
        assert_eq!(
            (c.title.as_str(), c.version.as_str()),
            ("Cadastral Map", "1.3.0")
        );
        assert_eq!(c.crs, "EPSG:3857");
        assert_eq!(c.url, "https://maps.example.org/wms?map=x&");
        assert_eq!(c.layers.len(), 3);
        assert!(c.layers[0].name.is_empty() && c.layers[1].depth == 1);
        // nothing chosen: nothing to add
        let mut st = Adding {
            checked: vec![false; 3],
            caps: Some(c),
            ..Default::default()
        };
        assert!(chosen(&st).is_empty());
        st.checked[2] = true;
        st.name = "Base".into();
        let l = chosen(&st).remove(0);
        assert_eq!(
            (l.name.as_str(), l.group.as_str(), l.layers.clone()),
            ("Roads", "Base", vec!["roads".to_string()])
        );
        let u = l.image_url([1.0, 2.0, 3.0, 4.0], 10, 20);
        assert_eq!(
            u,
            "https://maps.example.org/wms?map=x&SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS=roads\
             &STYLES=&FORMAT=image%2Fpng&TRANSPARENT=TRUE&WIDTH=10&HEIGHT=20&CRS=EPSG%3A3857&BBOX=1,2,3,4"
        );
        // longitude/latitude only: latitude first in WMS 1.3.0, not in 1.1.1
        let xml = WMS.replace("<CRS>EPSG:3857</CRS>", "");
        let mut l = parse_wms(&xml, url)
            .map(|c| MapLayer {
                crs: c.crs,
                url: c.url,
                version: c.version,
                layers: vec!["roads".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(l.crs, "EPSG:4326");
        assert!(
            l.image_url([30.0, 35.0, 31.0, 36.0], 10, 10)
                .ends_with("&BBOX=35,30,36,31")
        );
        l.version = "1.1.1".into();
        let u = l.image_url([30.0, 35.0, 31.0, 36.0], 10, 10);
        assert!(
            u.contains("&SRS=EPSG%3A4326") && u.ends_with("&BBOX=30,35,31,36"),
            "{u}"
        );
        // a service that answers with an error
        let err = r#"<ServiceExceptionReport><ServiceException>Bad request</ServiceException></ServiceExceptionReport>"#;
        assert_eq!(parse_wms(err, url).unwrap_err(), "Bad request");
    }

    #[test]
    fn asks_for_the_area_in_view() {
        let view = crate::map::MapView::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0));
        let geo = Geo::WebMercator;
        let (b, w, h) = wanted_area(&geo, &view, rect, 1.0, true).unwrap();
        assert_eq!((w, h), (800, 600));
        assert!(((b[2] - b[0]) / (b[3] - b[1]) - 800.0 / 600.0).abs() < 1e-6);
        // a sharp screen: no more than the largest picture
        let (_, w, h) = wanted_area(&geo, &view, rect, 4.0, true).unwrap();
        assert!(w <= 2048 && h <= 2048 && w > h, "{w} {h}");
        // a picture lands where it was asked for
        let mesh = picture_mesh(
            &geo,
            &view,
            rect,
            b,
            true,
            egui::TextureId::default(),
            Color32::WHITE,
        )
        .unwrap();
        let first = mesh.vertices.first().unwrap().pos;
        let last = mesh.vertices.last().unwrap().pos;
        assert!(
            first.x <= 1.0 && first.y <= 1.0 && last.x >= 799.0 && last.y >= 599.0,
            "{first:?} {last:?}"
        );
    }

    #[test]
    fn suggests_a_group_name_from_the_address() {
        let caps = |service, url: &str, title: &str| Caps {
            service,
            url: url.into(),
            title: title.into(),
            ..Default::default()
        };
        let arcgis = caps(
            Service::ArcGis,
            "https://eservices.dls.moi.gov.cy/arcgis/rest/services/National/CadastralMap_GR/MapServer",
            "ΤΕΜΑΧΙΑ",
        );
        assert_eq!(suggested_group(&arcgis), "Cadastral Map GR");
        let wms = caps(
            Service::Wms,
            "https://maps.example.org/geoserver/water_networks/wms?",
            "Water Networks of Cyprus",
        );
        assert_eq!(suggested_group(&wms), "Water Networks of Cyprus");
        let plain = caps(
            Service::Wms,
            "https://maps.example.org/geoserver/water_networks/wms?",
            "WMS",
        );
        assert_eq!(suggested_group(&plain), "water networks");
        let bare = caps(Service::Wms, "https://maps.example.org/wms", "");
        assert_eq!(suggested_group(&bare), "maps.example.org");
        assert_eq!(pretty("WaterNetworks-2024_v2"), "Water Networks 2024 v2");
    }

    /// A layer and a group are renamed in the panel: double-click, type, Enter.
    #[test]
    fn layers_and_groups_are_renamed() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        for name in ["Parcels", "Roads"] {
            app.prefs.map_layers.push(MapLayer {
                name: name.into(),
                group: "Cadastre".into(),
                url: "https://x/MapServer".into(),
                service: Service::ArcGis,
                layers: vec![name.into()],
                ..Default::default()
            });
        }
        // time goes on, so that two renames are not one long click series
        let time = std::cell::Cell::new(0.0);
        let run = |app: &mut EpanetApp, events: Vec<egui::Event>| {
            time.set(time.get() + 0.1);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(320.0, 600.0))),
                events,
                time: Some(time.get()),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| pane(app, ui));
            out.textures_delta.clear();
            out.shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    Shape::Text(t) => Some((t.galley.text().to_string(), t.visual_bounding_rect())),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let click = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let rename = |app: &mut EpanetApp, from: &str, to: &str| {
            let t = run(app, vec![]);
            let at = t.iter().find(|x| x.0 == from).unwrap().1.center();
            run(app, vec![egui::Event::PointerMoved(at)]);
            for _ in 0..2 {
                run(app, vec![click(at, true)]);
                run(app, vec![click(at, false)]);
            }
            run(app, vec![]);
            run(
                app,
                vec![
                    egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::COMMAND,
                    },
                    egui::Event::Text(to.into()),
                ],
            );
            run(
                app,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            run(app, vec![]);
        };
        rename(&mut app, "Roads", "Streets");
        assert_eq!(app.prefs.map_layers[1].name, "Streets");
        rename(&mut app, "Cadastre", "Land Registry");
        assert!(
            app.prefs
                .map_layers
                .iter()
                .all(|l| l.group == "Land Registry")
        );
        let t = run(&mut app, vec![]);
        assert!(t.iter().any(|x| x.0 == "Land Registry"));
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("a b,ΤΕΜ:1"), "a%20b,%CE%A4%CE%95%CE%9C%3A1");
        assert_eq!(
            without_params(
                "https://x/wms?Service=WMS&map=1&request=GetMap",
                &["service", "request"]
            ),
            "https://x/wms?map=1"
        );
        assert_eq!(with_query("https://x/wms"), "https://x/wms?");
    }
}
