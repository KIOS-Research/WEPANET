//! Basemaps drawn under the network: web map tiles (OpenStreetMap, CARTO,
//! Esri World Imagery) or an image file.
//!
//! Web tiles are in Web Mercator, so they need the network's coordinate
//! system. Networks in longitude/latitude are recognized by themselves; for
//! projected coordinates the user picks an EPSG code (or gives a proj string),
//! remembered per file name. Each tile is drawn as a small mesh whose corners
//! go through the coordinate transformation, so it lines up in any projection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Shape, Ui, pos2, vec2};
use proj4rs::proj::Proj;

use crate::app::EpanetApp;
use crate::io::Inbox;

/// Where the basemap comes from: EPANET-UI's four internet map providers or
/// an image file. (Earlier names are accepted from saved settings.)
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum BasemapKind {
    #[default]
    None,
    #[serde(alias = "Light")]
    WorldStreets,
    #[serde(alias = "Street")]
    OpenStreetMap,
    BingRoads,
    #[serde(alias = "Satellite")]
    BingSatellite,
    Image,
}

impl BasemapKind {
    pub const WEB: [BasemapKind; 4] = [
        BasemapKind::WorldStreets,
        BasemapKind::OpenStreetMap,
        BasemapKind::BingRoads,
        BasemapKind::BingSatellite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BasemapKind::None => "None",
            BasemapKind::WorldStreets => "World Streets",
            BasemapKind::OpenStreetMap => "OpenStreetMap",
            BasemapKind::BingRoads => "Bing Roads",
            BasemapKind::BingSatellite => "Bing Satellite",
            BasemapKind::Image => "Image File",
        }
    }

    fn url(self, z: u32, x: u32, y: u32) -> String {
        match self {
            BasemapKind::WorldStreets => format!(
                "https://server.arcgisonline.com/ArcGIS/rest/services/World_Street_Map/MapServer/tile/{z}/{y}/{x}"
            ),
            BasemapKind::OpenStreetMap => {
                format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png")
            }
            BasemapKind::BingRoads => format!(
                "https://ecn.t{}.tiles.virtualearth.net/tiles/r{}.png?g=1&mkt=en-us&shading=hill",
                (x + y) % 4,
                quadkey(z, x, y)
            ),
            _ => format!(
                "https://ecn.t{}.tiles.virtualearth.net/tiles/a{}.jpeg?g=1",
                (x + y) % 4,
                quadkey(z, x, y)
            ),
        }
    }

    fn attribution(self) -> &'static str {
        match self {
            BasemapKind::WorldStreets => "Tiles © Esri",
            BasemapKind::OpenStreetMap => "© OpenStreetMap contributors",
            BasemapKind::BingRoads | BasemapKind::BingSatellite => "© Microsoft Bing",
            _ => "",
        }
    }

    fn is_web(self) -> bool {
        Self::WEB.contains(&self)
    }
}

/// Bing's tile address: the tile's quadtree key.
fn quadkey(z: u32, x: u32, y: u32) -> String {
    (1..=z)
        .rev()
        .map(|i| {
            let m = 1 << (i - 1);
            let d = u32::from(x & m != 0) + 2 * u32::from(y & m != 0);
            char::from(b'0' + d as u8)
        })
        .collect()
}

/// Highest zoom level the tile servers provide.
const MAX_ZOOM: u32 = 19;
/// Tiles downloading at the same time.
const MAX_IN_FLIGHT: usize = 8;
/// Tiles kept in memory.
const CACHE_SIZE: usize = 400;

// ----- coordinate systems ---------------------------------------------------

/// Coordinate systems offered by EPSG code. Others can be given as a proj string.
const KNOWN_CRS: &[(u32, &str, &str)] = &[
    (
        4326,
        "WGS 84, longitude/latitude",
        "+proj=longlat +datum=WGS84 +no_defs",
    ),
    (3857, "WGS 84 / Pseudo-Mercator (web maps)", ""),
    (
        2100,
        "GGRS87 / Greek Grid (ΕΓΣΑ87)",
        "+proj=tmerc +lat_0=0 +lon_0=24 +k=0.9996 +x_0=500000 +y_0=0 +ellps=GRS80 \
         +towgs84=-199.87,74.79,246.62,0,0,0,0 +units=m +no_defs",
    ),
    (
        6312,
        "CGRS93 / Cyprus Local Transverse Mercator",
        "+proj=tmerc +lat_0=0 +lon_0=33 +k=0.99995 +x_0=200000 +y_0=-3500000 +ellps=GRS80 \
         +towgs84=8.846,-4.394,-1.122,-0.00237,-0.146528,0.130428,0.783926 +units=m +no_defs",
    ),
    (
        27700,
        "OSGB36 / British National Grid",
        "+proj=tmerc +lat_0=49 +lon_0=-2 +k=0.9996012717 +x_0=400000 +y_0=-100000 +ellps=airy \
         +towgs84=446.448,-125.157,542.06,0.15,0.247,0.842,-20.489 +units=m +no_defs",
    ),
];

/// The proj string of an EPSG code: the list above, plus every UTM zone
/// (WGS 84: 326xx north, 327xx south; ETRS89: 258xx; NAD83: 269xx).
fn epsg_proj(code: u32) -> Option<String> {
    if let Some((_, _, p)) = KNOWN_CRS.iter().find(|c| c.0 == code) {
        return Some(p.to_string());
    }
    let zone = code % 100;
    let utm = |extra: &str| {
        (1..=60)
            .contains(&zone)
            .then(|| format!("+proj=utm +zone={zone} {extra} +units=m +no_defs"))
    };
    match code / 100 {
        326 => utm("+datum=WGS84"),
        327 => utm("+south +datum=WGS84"),
        258 => utm("+ellps=GRS80 +towgs84=0,0,0,0,0,0,0"),
        269 => utm("+datum=NAD83"),
        _ => None,
    }
}

/// A short description of an EPSG code, for the settings dialog.
fn epsg_name(code: u32) -> Option<String> {
    if let Some((_, name, _)) = KNOWN_CRS.iter().find(|c| c.0 == code) {
        return Some(name.to_string());
    }
    let zone = code % 100;
    match code / 100 {
        326 => Some(format!("WGS 84 / UTM zone {zone}N")),
        327 => Some(format!("WGS 84 / UTM zone {zone}S")),
        258 => Some(format!("ETRS89 / UTM zone {zone}N")),
        269 => Some(format!("NAD83 / UTM zone {zone}N")),
        _ => None,
    }
    .filter(|_| epsg_proj(code).is_some())
}

/// Converts network coordinates to and from longitude/latitude (degrees).
pub enum Geo {
    LonLat,
    WebMercator,
    Proj(Box<Proj>, Box<Proj>),
}

const EARTH_R: f64 = 6378137.0;

impl Geo {
    /// From the text kept in the preferences: "EPSG:nnnn" or a proj string.
    pub(crate) fn parse(def: &str) -> Result<Geo, String> {
        let def = def.trim();
        let proj = match def
            .strip_prefix("EPSG:")
            .or_else(|| def.strip_prefix("epsg:"))
        {
            Some(code) => {
                let code: u32 = code
                    .trim()
                    .parse()
                    .map_err(|_| "Not an EPSG code".to_string())?;
                match code {
                    4326 => return Ok(Geo::LonLat),
                    3857 | 900913 => return Ok(Geo::WebMercator),
                    _ => epsg_proj(code).ok_or(format!(
                        "EPSG:{code} is not in the list; enter its proj string"
                    ))?,
                }
            }
            None => def.to_string(),
        };
        let src = Proj::from_proj_string(&proj).map_err(|e| format!("{e}"))?;
        if src.is_latlong() {
            return Ok(Geo::LonLat);
        }
        let wgs = Proj::from_proj_string("+proj=longlat +datum=WGS84 +no_defs")
            .map_err(|e| format!("{e}"))?;
        Ok(Geo::Proj(Box::new(src), Box::new(wgs)))
    }

    pub fn to_lonlat(&self, [x, y]: [f64; 2]) -> Option<[f64; 2]> {
        match self {
            Geo::LonLat => Some([x, y]),
            Geo::WebMercator => Some([
                (x / EARTH_R).to_degrees(),
                (y / EARTH_R).sinh().atan().to_degrees(),
            ]),
            Geo::Proj(src, wgs) => {
                let mut p = (x, y, 0.0);
                proj4rs::transform::transform(src, wgs, &mut p).ok()?;
                (p.0.is_finite() && p.1.is_finite()).then(|| [p.0.to_degrees(), p.1.to_degrees()])
            }
        }
    }

    pub fn lonlat_to(&self, [lon, lat]: [f64; 2]) -> Option<[f64; 2]> {
        match self {
            Geo::LonLat => Some([lon, lat]),
            Geo::WebMercator => {
                let lat = lat.to_radians();
                Some([EARTH_R * lon.to_radians(), EARTH_R * lat.tan().asinh()])
            }
            Geo::Proj(src, wgs) => {
                let mut p = (lon.to_radians(), lat.to_radians(), 0.0);
                proj4rs::transform::transform(wgs, src, &mut p).ok()?;
                (p.0.is_finite() && p.1.is_finite()).then_some([p.0, p.1])
            }
        }
    }
}

/// Whether coordinates within `b` can only be longitude/latitude.
fn looks_lonlat(b: [f64; 4]) -> bool {
    b[0] >= -180.0 && b[2] <= 180.0 && b[1] >= -90.0 && b[3] <= 90.0
}

/// Longitude/latitude to Web Mercator tile space (0..1 across the world).
fn to_unit([lon, lat]: [f64; 2]) -> [f64; 2] {
    let lat = lat.clamp(-85.05112878, 85.05112878).to_radians();
    [
        (lon + 180.0) / 360.0,
        (1.0 - lat.tan().asinh() / std::f64::consts::PI) / 2.0,
    ]
}

fn from_unit([u, v]: [f64; 2]) -> [f64; 2] {
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * v)).sinh().atan();
    [u * 360.0 - 180.0, lat.to_degrees()]
}

// ----- state ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileKey {
    kind: BasemapKind,
    gray: bool,
    z: u32,
    x: u32,
    y: u32,
}

enum Tile {
    Loading,
    Ready(egui::TextureHandle, u64),
    Failed,
}

type Downloaded = (TileKey, Result<egui::ColorImage, String>);

/// An image file used as a basemap, stretched over `bounds` (network coordinates).
pub struct BasemapImage {
    name: String,
    texture: egui::TextureHandle,
    pub bounds: [f64; 4],
    /// Width and height in pixels.
    pub size: [usize; 2],
    /// The image as loaded, for switching Grayscale on and off.
    original: egui::ColorImage,
}

impl BasemapImage {
    /// Shows the image in gray or in its colors.
    fn set_gray(&mut self, ctx: &egui::Context, gray: bool) {
        let mut img = self.original.clone();
        if gray {
            for p in img.pixels.iter_mut() {
                let l = (0.299 * p.r() as f32 + 0.587 * p.g() as f32 + 0.114 * p.b() as f32) as u8;
                *p = Color32::from_rgba_unmultiplied(l, l, l, p.a());
            }
        }
        self.texture = ctx.load_texture("basemap-image", img, egui::TextureOptions::LINEAR);
    }
}

#[derive(Default)]
pub struct BasemapState {
    tiles: HashMap<TileKey, Tile>,
    downloads: Arc<Mutex<Vec<Downloaded>>>,
    in_flight: usize,
    frame: u64,
    /// The coordinate system definition `geo` was made from.
    geo: Option<(String, Result<Geo, String>)>,
    pub image: Option<BasemapImage>,
    image_inbox: Inbox,
    /// EPSG code being typed in the source dialog.
    crs_edit: Option<String>,
    units_edit: usize,
    source_error: Option<String>,
    pub georef: Option<Georef>,
    pub align: Option<Align>,
    world_inbox: WorldFileInbox,
}

/// The coordinate system definition in effect: the one chosen for this file,
/// else longitude/latitude when the coordinates can only be that.
fn crs_def(app: &EpanetApp) -> Option<String> {
    if let Some(d) = app.prefs.basemap_crs.get(&app.file_name) {
        return Some(d.clone());
    }
    app.net
        .bounds()
        .filter(|&b| looks_lonlat(b))
        .map(|_| "EPSG:4326".to_string())
}

/// The network's coordinate transformation, if it is known.
pub fn geo(app: &mut EpanetApp) -> Result<&Geo, String> {
    let Some(def) = crs_def(app) else {
        return Err("The map's coordinate system is not set".into());
    };
    let st = &mut app.basemap;
    if st.geo.as_ref().is_none_or(|(d, _)| *d != def) {
        st.geo = Some((def.clone(), Geo::parse(&def)));
    }
    match &st.geo {
        Some((_, Ok(g))) => Ok(g),
        Some((_, Err(e))) => Err(e.clone()),
        None => unreachable!(),
    }
}

/// Starts using `kind` as the basemap.
pub fn choose(app: &mut EpanetApp, ctx: &egui::Context, kind: BasemapKind) {
    if kind == BasemapKind::Image {
        crate::io::open_file(
            &app.basemap.image_inbox,
            ctx,
            "Image",
            &["png", "jpg", "jpeg"],
        );
        return;
    }
    app.prefs.basemap = kind;
    app.prefs.show_basemap = true;
    if kind.is_web() && crs_def(app).is_none() {
        app.status = "Set the map's coordinate system to show a web basemap".into();
        app.dialog = crate::app::Dialog::Basemap;
    }
}

/// Handles finished downloads and loaded image files.
fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let st = &mut app.basemap;
    let done: Vec<Downloaded> = st
        .downloads
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default();
    for (key, res) in done {
        st.in_flight = st.in_flight.saturating_sub(1);
        let tile = match res {
            Ok(img) => {
                let name = format!("tile-{:?}-{}-{}-{}", key.kind, key.z, key.x, key.y);
                Tile::Ready(
                    ctx.load_texture(name, img, egui::TextureOptions::LINEAR),
                    st.frame,
                )
            }
            Err(e) => {
                log::warn!("basemap tile {key:?}: {e}");
                Tile::Failed
            }
        };
        st.tiles.insert(key, tile);
    }
    for item in st.image_inbox.drain() {
        match item.and_then(|(name, bytes)| decode(&bytes, false).map(|img| (name, img))) {
            Ok((name, img)) => {
                // across the network's width, keeping the picture's proportions
                let b = app.net.bounds().unwrap_or([0.0, 0.0, 10000.0, 10000.0]);
                let size = img.size;
                let s = (b[2] - b[0]).max(1e-9) / size[0].max(1) as f64;
                let bounds = [b[0], b[1], b[2], b[1] + s * size[1] as f64];
                let texture =
                    ctx.load_texture("basemap-image", img.clone(), egui::TextureOptions::LINEAR);
                app.status = format!(
                    "Basemap image {name} placed over the network (Georeference to place it exactly)"
                );
                let mut image = BasemapImage {
                    name,
                    texture,
                    bounds,
                    size,
                    original: img,
                };
                if app.prefs.basemap_gray {
                    image.set_gray(ctx, true);
                }
                app.basemap.image = Some(image);
                app.prefs.basemap = BasemapKind::Image;
                app.prefs.show_basemap = true;
            }
            Err(e) => app.status = format!("Could not open the image: {e}"),
        }
    }
}

fn decode(bytes: &[u8], gray: bool) -> Result<egui::ColorImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let mut rgba = img.to_rgba8();
    if gray {
        for p in rgba.pixels_mut() {
            let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
            p[0] = l;
            p[1] = l;
            p[2] = l;
        }
    }
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

fn request(st: &mut BasemapState, ctx: &egui::Context, key: TileKey) {
    st.tiles.insert(key, Tile::Loading);
    st.in_flight += 1;
    let downloads = st.downloads.clone();
    let ctx = ctx.clone();
    #[allow(unused_mut)]
    let mut req = ehttp::Request::get(key.kind.url(key.z, key.x, key.y));
    // OpenStreetMap asks applications to identify themselves (browsers send
    // their own User-Agent and do not allow a custom one).
    #[cfg(not(target_arch = "wasm32"))]
    req.headers
        .insert("User-Agent", "WEPANET/0.1 (water network modeling)");
    ehttp::fetch(req, move |res| {
        let img = match res {
            Ok(r) if r.ok => decode(&r.bytes, key.gray),
            Ok(r) => Err(format!("{} {}", r.status, r.status_text)),
            Err(e) => Err(e),
        };
        if let Ok(mut q) = downloads.lock() {
            q.push((key, img));
        }
        ctx.request_repaint();
    });
}

// ----- drawing --------------------------------------------------------------

/// Draws the basemap over `rect`, under the network.
pub fn draw(app: &mut EpanetApp, ui: &Ui, painter: &egui::Painter, rect: Rect, map_bg: Color32) {
    let ctx = ui.ctx().clone();
    poll(app, &ctx);
    let kind = app.prefs.basemap;
    if kind == BasemapKind::None || !app.prefs.show_basemap {
        return;
    }
    app.basemap.frame += 1;
    if kind == BasemapKind::Image {
        if let Some(img) = &app.basemap.image {
            let [x0, y0, x1, y1] = img.bounds;
            let r = Rect::from_two_pos(
                app.view.to_screen(rect, [x0, y0]),
                app.view.to_screen(rect, [x1, y1]),
            );
            painter.image(
                img.texture.id(),
                r,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    } else if let Err(e) = draw_tiles(app, &ctx, painter, rect, kind) {
        painter.text(
            rect.left_bottom() + vec2(8.0, -8.0),
            Align2::LEFT_BOTTOM,
            format!("Basemap: {e} (Basemap ▾ > Settings)"),
            FontId::proportional(12.0),
            Color32::from_rgb(0xc0, 0x39, 0x2b),
        );
        return;
    }
    let fade = app.prefs.basemap_fade.clamp(0.0, 0.95);
    if fade > 0.0 {
        painter.rect_filled(rect, 0.0, map_bg.gamma_multiply(fade));
    }
    let attribution = kind.attribution();
    if !attribution.is_empty() {
        let galley = painter.layout_no_wrap(
            attribution.to_string(),
            FontId::proportional(10.5),
            Color32::from_gray(60),
        );
        let r =
            Align2::RIGHT_BOTTOM.anchor_size(rect.right_bottom() - vec2(2.0, 2.0), galley.size());
        painter.rect_filled(r.expand(2.0), 0.0, Color32::from_white_alpha(200));
        painter.galley(r.min, galley, Color32::from_gray(60));
    }
}

fn draw_tiles(
    app: &mut EpanetApp,
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: Rect,
    kind: BasemapKind,
) -> Result<(), String> {
    let view = app.view.clone();
    let gray = app.prefs.basemap_gray;
    let geo = geo(app)?;
    // tile-space coordinates of a screen point
    let unit_at = |p: Pos2| geo.to_lonlat(view.to_world(rect, p)).map(to_unit);

    // the visible area, from a grid of screen points
    let (mut u0, mut v0, mut u1, mut v1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for i in 0..=4 {
        for j in 0..=4 {
            let p = rect.min
                + vec2(
                    rect.width() * i as f32 / 4.0,
                    rect.height() * j as f32 / 4.0,
                );
            if let Some([u, v]) = unit_at(p) {
                u0 = u0.min(u);
                v0 = v0.min(v);
                u1 = u1.max(u);
                v1 = v1.max(v);
            }
        }
    }
    if u0 > u1 {
        return Err("the network's coordinates do not fit the coordinate system".into());
    }
    // A zoom level where one tile pixel is about one screen pixel.
    let c = rect.center();
    let (Some(a), Some(b)) = (unit_at(c), unit_at(c + vec2(100.0, 0.0))) else {
        return Err("the network's coordinates do not fit the coordinate system".into());
    };
    let per_point = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() / 100.0;
    let ppp = ctx.pixels_per_point() as f64;
    let mut z = (ppp / (256.0 * per_point.max(1e-12)))
        .log2()
        .round()
        .clamp(0.0, MAX_ZOOM as f64) as u32;
    let range = |z: u32| {
        let n = (1u64 << z) as f64;
        let t = |x: f64| (x.clamp(0.0, 1.0) * n).floor().min(n - 1.0) as u32;
        (t(u0), t(v0), t(u1), t(v1))
    };
    let mut r = range(z);
    while z > 0 && ((r.2 - r.0 + 1) * (r.3 - r.1 + 1)) > 80 {
        z -= 1;
        r = range(z);
    }
    let (tx0, ty0, tx1, ty1) = r;

    // nearest tiles first, so downloads fill in from the middle
    let mut wanted: Vec<(u32, u32)> = (tx0..=tx1)
        .flat_map(|x| (ty0..=ty1).map(move |y| (x, y)))
        .collect();
    let n = (1u64 << z) as f64;
    let mid = [(a[0] * n), (a[1] * n)];
    wanted.sort_by(|p, q| {
        let d = |t: &(u32, u32)| {
            (t.0 as f64 + 0.5 - mid[0]).powi(2) + (t.1 as f64 + 0.5 - mid[1]).powi(2)
        };
        d(p).total_cmp(&d(q))
    });

    let frame = app.basemap.frame;
    let st = &mut app.basemap;
    let mut meshes = Vec::new();
    for (x, y) in wanted {
        let key = TileKey {
            kind,
            gray,
            z,
            x,
            y,
        };
        match st.tiles.get_mut(&key) {
            Some(Tile::Ready(tex, used)) => {
                *used = frame;
                meshes.push((tex.id(), key, [0.0, 0.0, 1.0, 1.0]));
                continue;
            }
            None if st.in_flight < MAX_IN_FLIGHT => request(st, ctx, key),
            _ => {}
        }
        // Meanwhile, part of a lower zoom tile stands in.
        for up in 1..=6.min(z) {
            let k = TileKey {
                z: z - up,
                x: x >> up,
                y: y >> up,
                ..key
            };
            if let Some(Tile::Ready(tex, used)) = st.tiles.get_mut(&k) {
                *used = frame;
                let s = 1.0 / (1u32 << up) as f64;
                let (ox, oy) = ((x - (k.x << up)) as f64 * s, (y - (k.y << up)) as f64 * s);
                meshes.push((tex.id(), key, [ox, oy, ox + s, oy + s]));
                break;
            }
        }
    }
    // stand-ins first, so the sharp tiles cover them
    meshes.sort_by_key(|m| m.2 == [0.0, 0.0, 1.0, 1.0]);
    for (tex, key, uv) in meshes {
        if let Some(mesh) = tile_mesh(geo_ref(app)?, &view, rect, key, uv, tex) {
            painter.add(Shape::mesh(mesh));
        }
    }
    evict(&mut app.basemap);
    Ok(())
}

/// The coordinate transformation already made by [`geo`].
fn geo_ref(app: &EpanetApp) -> Result<&Geo, String> {
    match &app.basemap.geo {
        Some((_, Ok(g))) => Ok(g),
        _ => Err("no coordinate system".into()),
    }
}

/// A tile as a 4x4 grid of quads whose corners go through the transformation.
fn tile_mesh(
    geo: &Geo,
    view: &crate::map::MapView,
    rect: Rect,
    key: TileKey,
    uv: [f64; 4],
    tex: egui::TextureId,
) -> Option<egui::Mesh> {
    const N: u32 = 4;
    let n = (1u64 << key.z) as f64;
    let mut mesh = egui::Mesh::with_texture(tex);
    for j in 0..=N {
        for i in 0..=N {
            let (fx, fy) = (i as f64 / N as f64, j as f64 / N as f64);
            let unit = [(key.x as f64 + fx) / n, (key.y as f64 + fy) / n];
            let world = geo.lonlat_to(from_unit(unit))?;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: view.to_screen(rect, world),
                uv: pos2(
                    (uv[0] + (uv[2] - uv[0]) * fx) as f32,
                    (uv[1] + (uv[3] - uv[1]) * fy) as f32,
                ),
                color: Color32::WHITE,
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

/// Drops the least recently drawn tiles once the cache is full.
fn evict(st: &mut BasemapState) {
    if st.tiles.len() <= CACHE_SIZE {
        return;
    }
    let mut ready: Vec<(TileKey, u64)> = st
        .tiles
        .iter()
        .filter_map(|(k, t)| match t {
            Tile::Ready(_, used) => Some((*k, *used)),
            Tile::Failed => Some((*k, 0)),
            Tile::Loading => None,
        })
        .collect();
    ready.sort_by_key(|r| r.1);
    for (k, _) in ready.into_iter().take(st.tiles.len() - CACHE_SIZE) {
        st.tiles.remove(&k);
    }
}

/// Map units of the Basemap source dialog (as Project Setup's Map page).
const UNITS: [&str; 4] = ["None", "Feet", "Meters", "Degrees"];

/// A tile of a provider for its button in the source dialog (a fixed place,
/// as EPANET-UI's pictures), once downloaded.
fn thumbnail(
    app: &mut EpanetApp,
    ctx: &egui::Context,
    kind: BasemapKind,
) -> Option<egui::TextureId> {
    const Z: u32 = 15;
    let [u, v] = to_unit([-122.0537, 38.2561]);
    let n = (1u32 << Z) as f64;
    let key = TileKey {
        kind,
        gray: false,
        z: Z,
        x: (u * n) as u32,
        y: (v * n) as u32,
    };
    let st = &mut app.basemap;
    match st.tiles.get(&key) {
        Some(Tile::Ready(t, _)) => Some(t.id()),
        Some(_) => None,
        None => {
            request(st, ctx, key);
            None
        }
    }
}

/// EPANET-UI's "Select a Basemap Source": an image file, or an internet map
/// with the coordinate system (EPSG code and units) of the network.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    poll(app, &ctx);
    ui.set_width(440.0);
    let empty = app.net.nodes.is_empty();
    if app.basemap.crs_edit.is_none() {
        // EPANET-UI: an empty project is laid out in longitude/latitude
        let saved = app
            .prefs
            .basemap_crs
            .get(&app.file_name)
            .and_then(|c| c.strip_prefix("EPSG:").map(str::to_string));
        let units = app
            .net
            .option("BACKDROP", "UNITS")
            .and_then(|u| UNITS.iter().position(|m| m.eq_ignore_ascii_case(&u)));
        let lonlat = app.net.bounds().is_some_and(looks_lonlat);
        app.basemap.crs_edit = Some(if empty || (saved.is_none() && lonlat) {
            "4326".into()
        } else {
            saved.unwrap_or_default()
        });
        app.basemap.units_edit = if empty || lonlat {
            3
        } else {
            units.unwrap_or(2)
        };
        app.basemap.source_error = None;
    }
    let group = |ui: &mut Ui, title: &str, add: &mut dyn FnMut(&mut Ui)| {
        ui.label(RichText::new(title).size(14.0));
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(200)))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui)
            });
    };
    let mut pick: Option<BasemapKind> = None;
    group(ui, "Local File System", &mut |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(170.0, 120.0), egui::Sense::click());
        let v = ui.visuals();
        ui.painter().rect(
            r,
            3.0,
            if resp.hovered() {
                v.widgets.hovered.weak_bg_fill
            } else {
                v.extreme_bg_color
            },
            egui::Stroke::new(1.0, crate::widgets::ACCENT),
            egui::StrokeKind::Inside,
        );
        // a small landscape picture
        let pic = Rect::from_center_size(r.center() - vec2(0.0, 14.0), vec2(74.0, 56.0));
        ui.painter()
            .rect_filled(pic, 0.0, Color32::from_rgb(0xbf, 0xe3, 0xfa));
        ui.painter().add(Shape::convex_polygon(
            vec![
                pic.left_bottom(),
                pic.left_bottom() + vec2(30.0, -36.0),
                pic.left_bottom() + vec2(56.0, 0.0),
            ],
            Color32::from_rgb(0x5a, 0x7f, 0xb5),
            egui::Stroke::NONE,
        ));
        ui.painter().rect_stroke(
            pic,
            0.0,
            egui::Stroke::new(1.5, Color32::from_gray(40)),
            egui::StrokeKind::Outside,
        );
        ui.painter().text(
            r.center() + vec2(0.0, 36.0),
            Align2::CENTER_CENTER,
            "Image File",
            FontId::proportional(14.0),
            v.text_color(),
        );
        if resp.clicked() {
            pick = Some(BasemapKind::Image);
        }
    });
    ui.add_space(10.0);
    let thumbs: Vec<_> = BasemapKind::WEB
        .iter()
        .map(|&k| (k, thumbnail(app, &ctx, k)))
        .collect();
    let st = &mut app.basemap;
    let edit = st.crs_edit.as_mut().unwrap();
    let units = &mut st.units_edit;
    group(ui, "Internet Map Providers", &mut |ui| {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!empty, |ui| {
                ui.vertical(|ui| {
                    ui.label("CRS EPSG");
                    ui.add(egui::TextEdit::singleline(edit).desired_width(120.0));
                    let name = edit.trim().parse().ok().and_then(epsg_name);
                    ui.label(RichText::new(name.unwrap_or_default()).weak().size(11.0));
                });
                ui.label(RichText::new(" ? ").color(crate::widgets::ACCENT).strong())
                    .on_hover_text(
                        "The EPSG code of the coordinate system the network's coordinates are in, \
                         e.g. 4326 for longitude/latitude, 2100 for the Greek Grid, 6312 for Cyprus, \
                         326zz for UTM zone zz north. Find it at https://epsg.io.",
                    );
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.label("CRS Units");
                    egui::ComboBox::from_id_salt("basemap_units")
                        .selected_text(UNITS[*units])
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for (k, u) in UNITS.iter().enumerate() {
                                ui.selectable_value(units, k, *u);
                            }
                        });
                });
            });
        });
        ui.add_space(10.0);
        egui::Grid::new("providers")
            .num_columns(2)
            .spacing([24.0, 20.0])
            .show(ui, |ui| {
                for (k, (kind, tex)) in thumbs.iter().enumerate() {
                    let (r, resp) =
                        ui.allocate_exact_size(vec2(170.0, 120.0), egui::Sense::click());
                    let v = ui.visuals();
                    ui.painter().rect(
                        r,
                        3.0,
                        if resp.hovered() {
                            v.widgets.hovered.weak_bg_fill
                        } else {
                            v.faint_bg_color
                        },
                        egui::Stroke::new(1.0, Color32::from_gray(215)),
                        egui::StrokeKind::Inside,
                    );
                    let pic =
                        Rect::from_center_size(r.center() - vec2(0.0, 14.0), vec2(74.0, 56.0));
                    match tex {
                        Some(t) => {
                            // the middle of the tile
                            let uv = Rect::from_min_max(pos2(0.2, 0.25), pos2(0.8, 0.7));
                            ui.painter().image(*t, pic, uv, Color32::WHITE);
                        }
                        None => {
                            ui.painter().rect_filled(pic, 0.0, Color32::from_gray(225));
                        }
                    }
                    ui.painter().rect_stroke(
                        pic,
                        0.0,
                        egui::Stroke::new(1.5, Color32::from_gray(40)),
                        egui::StrokeKind::Outside,
                    );
                    ui.painter().text(
                        r.center() + vec2(0.0, 36.0),
                        Align2::CENTER_CENTER,
                        kind.label(),
                        FontId::proportional(14.0),
                        v.text_color(),
                    );
                    if resp.clicked() {
                        pick = Some(*kind);
                    }
                    if k % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
    });
    if let Some(err) = &st.source_error {
        ui.label(RichText::new(err).color(Color32::from_rgb(0xc0, 0x39, 0x2b)));
    }
    let mut close = false;
    match pick {
        Some(BasemapKind::Image) => {
            choose(app, &ctx, BasemapKind::Image);
            close = true;
        }
        Some(kind) => {
            let code = app.basemap.crs_edit.clone().unwrap_or_default();
            let units = app.basemap.units_edit;
            let def = if units == 3 && code.trim().is_empty() {
                "EPSG:4326".to_string()
            } else {
                format!("EPSG:{}", code.trim())
            };
            match Geo::parse(&def) {
                Err(e) => {
                    app.basemap.source_error = Some(if code.trim().is_empty() {
                        "Enter the EPSG code of the network's coordinates".into()
                    } else {
                        e
                    })
                }
                Ok(_) => {
                    app.prefs.basemap_crs.insert(app.file_name.clone(), def);
                    app.net
                        .set_option("BACKDROP", "UNITS", &UNITS[units].to_ascii_uppercase());
                    app.prefs.basemap = kind;
                    app.prefs.show_basemap = true;
                    if empty {
                        // show the world until there is a network to fit
                        app.view.fit_to = Some([-180.0, -60.0, 180.0, 75.0]);
                        app.view.fit_pending = true;
                    }
                    close = true;
                }
            }
        }
        None => {}
    }
    if close {
        app.basemap.crs_edit = None;
    }
    close
}

/// The Basemap ▾ drop-down of the Map toolbar, as EPANET-UI's.
pub fn menu(app: &mut EpanetApp, ui: &mut Ui) {
    use crate::icons::{Icon, check_item, menu_item};
    let loaded = app.prefs.basemap != BasemapKind::None;
    let image = app.prefs.basemap == BasemapKind::Image && app.basemap.image.is_some();
    if menu_item(ui, Icon::Basemap, "Load", true).clicked() {
        app.basemap.crs_edit = None;
        app.dialog = crate::app::Dialog::Basemap;
        ui.close();
    }
    if menu_item(ui, Icon::Delete, "Unload", loaded).clicked() {
        app.prefs.basemap = BasemapKind::None;
        app.basemap.image = None;
        app.basemap.georef = None;
        app.basemap.align = None;
        ui.close();
    }
    ui.separator();
    if menu_item(ui, Icon::Locate, "Georeference", image).clicked() {
        start_georef(app);
        ui.close();
    }
    if menu_item(ui, Icon::Reshape, "Align", image).clicked() {
        start_align(app);
        ui.close();
    }
    ui.separator();
    let lighten = app.prefs.basemap_fade > 0.0;
    if check_item(ui, "Lighten", lighten).clicked() {
        app.prefs.basemap_fade = if lighten { 0.0 } else { 0.45 };
        ui.close();
    }
    if check_item(ui, "Grayscale", app.prefs.basemap_gray).clicked() {
        app.prefs.basemap_gray = !app.prefs.basemap_gray;
        let gray = app.prefs.basemap_gray;
        app.basemap.tiles.retain(|k, _| k.gray == gray);
        if let Some(img) = &mut app.basemap.image {
            img.set_gray(ui.ctx(), gray);
        }
        ui.close();
    }
}

// ----- georeference -----------------------------------------------------------

/// The Map Georeference Tool (EPANET-UI's `mapgeoref`): places the basemap
/// image in map coordinates from two control points and their distance plus
/// a point of known coordinates, from a scale and its lower left corner, or
/// from a world file.
#[derive(Clone, Debug, Default)]
pub struct Georef {
    method: usize,
    step: usize,
    /// Image pixel positions of the control points.
    points: [Option<[f64; 2]>; 3],
    distance: String,
    x: String,
    y: String,
    scale: String,
    llx: String,
    lly: String,
    error: Option<String>,
    original: [f64; 4],
}

#[derive(Default)]
pub struct WorldFileInbox(pub Inbox);

fn start_georef(app: &mut EpanetApp) {
    let Some(img) = &app.basemap.image else {
        return;
    };
    let b = img.bounds;
    let s = (b[2] - b[0]) / img.size[0].max(1) as f64;
    app.basemap.georef = Some(Georef {
        scale: format!("{s:.6}"),
        llx: format!("{:.6}", b[0]),
        lly: format!("{:.6}", b[1]),
        original: b,
        ..Default::default()
    });
    app.dialog = crate::app::Dialog::Georeference;
}

/// Whether the georeference tool is waiting for a click on the image.
fn picking(st: &BasemapState) -> bool {
    st.georef
        .as_ref()
        .is_some_and(|g| g.method == 0 && matches!(g.step, 0 | 1 | 3))
}

/// Takes a click on the map for the georeference tool; returns true if used.
pub fn map_click(app: &mut EpanetApp, world: [f64; 2]) -> bool {
    if !picking(&app.basemap) {
        return false;
    }
    let Some(img) = &app.basemap.image else {
        return false;
    };
    let b = img.bounds;
    let px = [
        (world[0] - b[0]) / (b[2] - b[0]) * img.size[0] as f64,
        (b[3] - world[1]) / (b[3] - b[1]) * img.size[1] as f64,
    ];
    let g = app.basemap.georef.as_mut().unwrap();
    let k = if g.step == 3 { 2 } else { g.step };
    g.points[k] = Some(px);
    g.step += 1;
    true
}

/// Places the image with `scale` world units per pixel and the given lower
/// left corner.
fn place(img: &mut BasemapImage, scale: f64, llx: f64, lly: f64) {
    img.bounds = [
        llx,
        lly,
        llx + scale * img.size[0] as f64,
        lly + scale * img.size[1] as f64,
    ];
}

/// Reads a world file (six lines: x scale, two rotation terms, -y scale, and
/// the map position of the upper left pixel's center).
fn parse_world_file(text: &str) -> Option<(f64, f64, f64)> {
    let v: Vec<f64> = text.lines().filter_map(|l| l.trim().parse().ok()).collect();
    (v.len() >= 6 && v[0] > 0.0).then(|| (v[0], v[4] - v[0] / 2.0, v[5] - v[3] / 2.0))
}

/// The Map Georeference Tool window.
pub fn georef_dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let ctx = ui.ctx().clone();
    for item in app.basemap.world_inbox.0.drain() {
        let text = item.map(|(_, b)| crate::io::decode_text(&b));
        let (Some(img), Some(g)) = (app.basemap.image.as_mut(), app.basemap.georef.as_mut()) else {
            continue;
        };
        match text.ok().as_deref().and_then(parse_world_file) {
            Some((s, left, top)) => {
                let h = s * img.size[1] as f64;
                place(img, s, left, top - h);
                g.scale = format!("{s:.6}");
                g.llx = format!("{left:.6}");
                g.lly = format!("{:.6}", top - h);
                g.error = None;
            }
            None => g.error = Some("That is not a world file".into()),
        }
    }
    let Some(g) = app.basemap.georef.as_mut() else {
        return true;
    };
    let Some(img) = app.basemap.image.as_mut() else {
        return true;
    };
    ui.set_width(380.0);
    ui.label(RichText::new(&img.name).strong());
    ui.label("Georeference the basemap using:");
    for (k, name) in [
        "control points + distance",
        "scaling factor + lower left",
        "a world file",
    ]
    .iter()
    .enumerate()
    {
        if crate::widgets::radio(ui, g.method == k, *name).clicked() {
            g.method = k;
            g.step = 0;
            g.error = None;
        }
    }
    ui.separator();
    let num = |s: &str| s.trim().parse::<f64>().ok();
    match g.method {
        0 => {
            let done = |p: &Option<[f64; 2]>| {
                p.map_or("—".to_string(), |p| {
                    format!("pixel {:.0}, {:.0}", p[0], p[1])
                })
            };
            ui.label(format!(
                "1. Click Control Point 1 on the basemap: {}",
                done(&g.points[0])
            ));
            ui.label(format!(
                "2. Click Control Point 2 on the basemap: {}",
                done(&g.points[1])
            ));
            ui.add_enabled_ui(g.step >= 2, |ui| {
                ui.horizontal(|ui| {
                    ui.label("3. Distance between the control points:");
                    ui.add(egui::TextEdit::singleline(&mut g.distance).desired_width(90.0));
                });
            });
            if g.step == 2
                && ui
                    .add_enabled(
                        num(&g.distance).is_some_and(|d| d > 0.0),
                        egui::Button::new("Next"),
                    )
                    .clicked()
            {
                g.step = 3;
            }
            ui.add_enabled_ui(g.step >= 3, |ui| {
                ui.label(format!(
                    "4. Click Control Point 3 on the basemap: {}",
                    done(&g.points[2])
                ));
                ui.horizontal(|ui| {
                    ui.label("   Its true world coordinates  X:");
                    ui.add(egui::TextEdit::singleline(&mut g.x).desired_width(90.0));
                    ui.label("Y:");
                    ui.add(egui::TextEdit::singleline(&mut g.y).desired_width(90.0));
                });
            });
            if matches!(g.step, 0 | 1 | 3) {
                ui.label(
                    RichText::new("Click the point on the map…").color(crate::widgets::ACCENT),
                );
            }
        }
        1 => {
            egui::Grid::new("georef_scale")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("World / Pixel");
                    ui.add(egui::TextEdit::singleline(&mut g.scale).desired_width(120.0));
                    ui.end_row();
                    ui.label("Lower Left X");
                    ui.add(egui::TextEdit::singleline(&mut g.llx).desired_width(120.0));
                    ui.end_row();
                    ui.label("Lower Left Y");
                    ui.add(egui::TextEdit::singleline(&mut g.lly).desired_width(120.0));
                    ui.end_row();
                });
        }
        _ => {
            if ui.button("Load a World File…").clicked() {
                crate::io::open_file(
                    &app.basemap.world_inbox.0,
                    &ctx,
                    "World file",
                    &["pgw", "jgw", "wld", "tfw", "pngw", "jpgw"],
                );
            }
            ui.label(RichText::new("A .pgw, .jgw or .wld file beside the image.").weak());
        }
    }
    ui.separator();
    let b = img.bounds;
    ui.label(format!(
        "Georeferenced basemap extents:\n  lower left {:.3}, {:.3}\n  upper right {:.3}, {:.3}",
        b[0], b[1], b[2], b[3]
    ));
    if let Some(e) = &g.error {
        ui.label(RichText::new(e).color(Color32::from_rgb(0xc0, 0x39, 0x2b)));
    }
    ui.add_space(6.0);
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button("Help").clicked() {
            ctx.open_url(egui::OpenUrl::new_tab(crate::menu::USER_MANUAL));
        }
        if ui.button("Apply").clicked() {
            match g.method {
                0 => match (g.points, num(&g.distance), num(&g.x), num(&g.y)) {
                    ([Some(p1), Some(p2), Some(p3)], Some(d), Some(x), Some(y)) if d > 0.0 => {
                        let px = ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt();
                        if px < 1.0 {
                            g.error = Some("Control points 1 and 2 are the same".into());
                        } else {
                            let s = d / px;
                            // p3 is (x, y); image rows run downward
                            let llx = x - p3[0] * s;
                            let lly = y - (img.size[1] as f64 - p3[1]) * s;
                            place(img, s, llx, lly);
                            g.error = None;
                        }
                    }
                    _ => g.error = Some(
                        "Locate the three control points and enter the distance and coordinates"
                            .into(),
                    ),
                },
                1 => match (num(&g.scale), num(&g.llx), num(&g.lly)) {
                    (Some(s), Some(x), Some(y)) if s > 0.0 => {
                        place(img, s, x, y);
                        g.error = None;
                    }
                    _ => {
                        g.error =
                            Some("Enter a positive scale and the lower left coordinates".into())
                    }
                },
                _ => {}
            }
        }
        if ui.button("OK").clicked() {
            close = true;
        }
        if ui.button("Cancel").clicked() {
            img.bounds = g.original;
            close = true;
        }
    });
    if close {
        app.basemap.georef = None;
    }
    close
}

// ----- align -------------------------------------------------------------------

/// The Map Alignment Tool (EPANET-UI's `mapalign`): drag the basemap image
/// with the mouse and stretch it to line it up with the network.
#[derive(Clone, Debug)]
pub struct Align {
    pub drag: bool,
    /// Scaling in percent (EPANET-UI's track bar).
    scale: f32,
    original: [f64; 4],
    /// The image's bounds at scale 100% after moving.
    base: [f64; 4],
}

fn start_align(app: &mut EpanetApp) {
    if let Some(img) = &app.basemap.image {
        app.basemap.align = Some(Align {
            drag: false,
            scale: 0.0,
            original: img.bounds,
            base: img.bounds,
        });
        app.dialog = crate::app::Dialog::BasemapAlign;
    }
}

/// Moves the image by `delta` (world units) when the Align tool drags it;
/// returns true if it did.
pub fn align_drag(app: &mut EpanetApp, delta: [f64; 2]) -> bool {
    let (Some(a), Some(img)) = (app.basemap.align.as_mut(), app.basemap.image.as_mut()) else {
        return false;
    };
    if !a.drag {
        return false;
    }
    for k in 0..4 {
        a.base[k] += delta[k % 2];
        img.bounds[k] += delta[k % 2];
    }
    true
}

/// The Map Alignment Tool window.
pub fn align_dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let (Some(a), Some(img)) = (app.basemap.align.as_mut(), app.basemap.image.as_mut()) else {
        return true;
    };
    ui.set_width(340.0);
    crate::widgets::checkbox(ui, &mut a.drag, "Drag basemap with mouse");
    ui.add_enabled_ui(a.drag, |ui| {
        ui.label(RichText::new("Drag the map to move the basemap under the network.").weak());
    });
    ui.add_space(6.0);
    ui.label(format!("Scale basemap: {:+.0} %", a.scale));
    let mut scale = a.scale as f64;
    let changed =
        crate::widgets::slider(ui, true, &mut scale, -50.0..=100.0, false, Some(1.0)).changed();
    a.scale = scale as f32;
    if changed {
        let f = 1.0 + a.scale as f64 / 100.0;
        let c = [(a.base[0] + a.base[2]) / 2.0, (a.base[1] + a.base[3]) / 2.0];
        let (hw, hh) = (
            (a.base[2] - a.base[0]) / 2.0 * f,
            (a.base[3] - a.base[1]) / 2.0 * f,
        );
        img.bounds = [c[0] - hw, c[1] - hh, c[0] + hw, c[1] + hh];
    }
    ui.add_space(8.0);
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button("Accept").clicked() {
            close = true;
        }
        if ui.button("Cancel").clicked() {
            img.bounds = a.original;
            close = true;
        }
    });
    if close {
        app.basemap.align = None;
    }
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_mercator_round_trip() {
        let g = Geo::WebMercator;
        let p = g.lonlat_to([33.38, 35.17]).unwrap();
        let q = g.to_lonlat(p).unwrap();
        assert!((q[0] - 33.38).abs() < 1e-9 && (q[1] - 35.17).abs() < 1e-9);
    }

    #[test]
    fn tile_space_round_trip() {
        let ll = from_unit(to_unit([23.72, 37.98]));
        assert!((ll[0] - 23.72).abs() < 1e-9 && (ll[1] - 37.98).abs() < 1e-9);
    }

    #[test]
    fn greek_grid_athens() {
        // Syntagma Square, Athens: about 476 km E, 4 204 km N in GGRS87.
        let g = Geo::parse("EPSG:2100").unwrap();
        let [lon, lat] = g.to_lonlat([476_000.0, 4_204_000.0]).unwrap();
        assert!((lon - 23.73).abs() < 0.03, "{lon}");
        assert!((lat - 37.98).abs() < 0.03, "{lat}");
        let back = g.lonlat_to([lon, lat]).unwrap();
        assert!((back[0] - 476_000.0).abs() < 0.01 && (back[1] - 4_204_000.0).abs() < 0.01);
    }

    #[test]
    fn cyprus_ltm_nicosia() {
        // Nicosia is near 33.36 E, 35.17 N.
        let g = Geo::parse("EPSG:6312").unwrap();
        let [x, y] = g.lonlat_to([33.36, 35.17]).unwrap();
        let [lon, lat] = g.to_lonlat([x, y]).unwrap();
        assert!((lon - 33.36).abs() < 1e-7 && (lat - 35.17).abs() < 1e-7);
        assert!((x - 232_800.0).abs() < 3_000.0, "{x}");
        assert!((y - 390_000.0).abs() < 15_000.0, "{y}");
    }

    /// Downloads and decodes one tile of each web basemap (needs the internet:
    /// `cargo test -p eui-ui -- --ignored`).
    #[test]
    #[ignore]
    fn download_tiles() {
        for kind in BasemapKind::WEB {
            let mut req = ehttp::Request::get(kind.url(12, 2427, 1612));
            req.headers
                .insert("User-Agent", "WEPANET/0.1 (water network modeling)");
            let res = ehttp::fetch_blocking(&req).expect("download");
            assert!(res.ok, "{kind:?}: {}", res.status);
            let img = decode(&res.bytes, true).expect("decode");
            assert_eq!(img.size, [256, 256], "{kind:?}");
        }
    }

    #[test]
    fn bing_quadkey() {
        // Microsoft's example: tile (3, 5) at level 3 is "213"
        assert_eq!(quadkey(3, 3, 5), "213");
        assert_eq!(quadkey(0, 0, 0), "");
    }

    #[test]
    fn world_file() {
        // 0.5 m pixels, upper left pixel centered at (1000.25, 2000.75)
        let (s, left, top) = parse_world_file(
            "0.5
0
0
-0.5
1000.25
2000.75
",
        )
        .unwrap();
        assert_eq!((s, left, top), (0.5, 1000.0, 2001.0));
        assert!(parse_world_file("nonsense").is_none());
    }

    #[test]
    fn utm_codes() {
        assert!(epsg_proj(32634).is_some());
        assert!(epsg_proj(32761).is_none());
        assert_eq!(epsg_name(32636).as_deref(), Some("WGS 84 / UTM zone 36N"));
    }
}
