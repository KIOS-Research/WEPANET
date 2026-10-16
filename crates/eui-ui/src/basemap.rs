//! Basemaps drawn under the network: web map tiles (OpenStreetMap, CARTO,
//! Esri World Imagery) or an image file.
//!
//! Web tiles are in Web Mercator, so they need the network's coordinate
//! system. Networks in longitude/latitude are recognized by themselves; for
//! projected coordinates the user picks an EPSG code (or gives a proj string),
//! remembered per file name. Each tile is drawn as a small mesh whose corners
//! go through the coordinate transformation, so it lines up in any projection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

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
    (
        3857,
        "WGS 84 / Pseudo-Mercator (web maps)",
        "+proj=merc +a=6378137 +b=6378137 +lat_ts=0 +lon_0=0 +x_0=0 +y_0=0 +k=1 +units=m +nadgrids=@null +wktext +no_defs",
    ),
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
        3088,
        "NAD83 / Kentucky Single Zone",
        "+proj=lcc +lat_1=37.08333333333334 +lat_2=38.66666666666666 +lat_0=36.33333333333334 \
         +lon_0=-85.75 +x_0=1500000 +y_0=1000000 +ellps=GRS80 +towgs84=0,0,0,0,0,0,0 +units=m +no_defs",
    ),
    (
        3089,
        "NAD83 / Kentucky Single Zone (ftUS)",
        "+proj=lcc +lat_1=37.08333333333334 +lat_2=38.66666666666666 +lat_0=36.33333333333334 \
         +lon_0=-85.75 +x_0=1500000 +y_0=1000000 +ellps=GRS80 +towgs84=0,0,0,0,0,0,0 +units=us-ft \
         +no_defs",
    ),
    (
        27700,
        "OSGB36 / British National Grid",
        "+proj=tmerc +lat_0=49 +lon_0=-2 +k=0.9996012717 +x_0=400000 +y_0=-100000 +ellps=airy \
         +towgs84=446.448,-125.157,542.06,0.15,0.247,0.842,-20.489 +units=m +no_defs",
    ),
];

/// The proj string of an EPSG code: the list above, else the EPSG database
/// (every coordinate system with a proj definition).
fn epsg_proj(code: u32) -> Option<String> {
    if let Some((_, _, p)) = KNOWN_CRS.iter().find(|c| c.0 == code) {
        return Some(p.to_string());
    }
    let def = crs_definitions::from_code(u16::try_from(code).ok()?)?;
    (!def.proj4.is_empty()).then(|| def.proj4.to_string())
}

/// A short description of an EPSG code, for the settings dialog: its name
/// from the list above or the EPSG database, else the projection and units.
fn epsg_name(code: u32) -> Option<String> {
    if let Some((_, name, _)) = KNOWN_CRS.iter().find(|c| c.0 == code) {
        return Some(name.to_string());
    }
    if let Some(name) = CRS_NAMES.lines().find_map(|l| {
        let (c, n) = l.split_once('\t')?;
        (c.parse() == Ok(code)).then_some(n)
    }) {
        return Some(name.to_string());
    }
    let zone = code % 100;
    let utm = match code / 100 {
        326 => Some(format!("WGS 84 / UTM zone {zone}N")),
        327 => Some(format!("WGS 84 / UTM zone {zone}S")),
        258 => Some(format!("ETRS89 / UTM zone {zone}N")),
        269 => Some(format!("NAD83 / UTM zone {zone}N")),
        _ => None,
    };
    let proj = epsg_proj(code)?;
    if utm.is_some() && proj.contains("+proj=utm") {
        return utm;
    }
    let param = |k: &str| {
        proj.split_whitespace()
            .find_map(|p| p.strip_prefix(k))
            .map(str::to_string)
    };
    let units = match param("+units=").as_deref() {
        Some("m") => "meters".to_string(),
        Some("us-ft") => "US feet".to_string(),
        Some("ft") => "feet".to_string(),
        Some(u) => u.to_string(),
        None if param("+proj=").as_deref() == Some("longlat") => "degrees".to_string(),
        None => "meters".to_string(),
    };
    Some(format!(
        "{} projection, {units}",
        param("+proj=").unwrap_or_default()
    ))
}

/// Names of the EPSG coordinate systems (code, tab, name; deprecated ones
/// left out), taken from the WKT definitions of the `crs-definitions` crate.
const CRS_NAMES: &str = include_str!("crs_names.tsv");

/// A coordinate system of the list: code, name, "epsg:code name" in
/// lower case for searching.
type CrsEntry = (u32, String, String);

/// The coordinate systems that can be used, for the list of the source dialog.
fn crs_list() -> &'static [CrsEntry] {
    static LIST: OnceLock<Vec<CrsEntry>> = OnceLock::new();
    LIST.get_or_init(|| {
        CRS_NAMES
            .lines()
            .filter_map(|l| {
                let (code, name) = l.split_once('\t')?;
                let code: u32 = code.parse().ok()?;
                Geo::parse(&format!("EPSG:{code}")).ok()?;
                let label = name.to_string();
                let lower = format!("epsg:{code} {name}").to_lowercase();
                Some((code, label, lower))
            })
            .collect()
    })
}

/// The CRS Units entry of an EPSG code.
fn epsg_units(code: u32) -> usize {
    let proj = epsg_proj(code).unwrap_or_default();
    if code == 4326 || proj.contains("+proj=longlat") {
        3
    } else if proj.contains("+units=us-ft") || proj.contains("+units=ft") {
        1
    } else {
        2
    }
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
                        "EPSG:{code} is not a known EPSG code; enter its proj string"
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

/// Whether `geo` turns the network's extent `b` into real longitudes and
/// latitudes, i.e. the network can be in that coordinate system.
fn fits(geo: &Geo, b: [f64; 4]) -> bool {
    [
        [b[0], b[1]],
        [b[2], b[3]],
        [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0],
    ]
    .into_iter()
    .all(|p| geo.to_lonlat(p).is_some_and(valid_lonlat))
}

fn valid_lonlat([lon, lat]: [f64; 2]) -> bool {
    (-180.0..=180.0).contains(&lon) && (-90.0..=90.0).contains(&lat)
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
    /// The Coordinate Reference System Selector, while open.
    selector: Option<CrsSelector>,
    source_error: Option<String>,
    pub georef: Option<Georef>,
    pub align: Option<Align>,
    world_inbox: WorldFileInbox,
}

impl BasemapState {
    /// Whether the Coordinate Reference System Selector is open.
    pub fn selector_open(&self) -> bool {
        self.selector.is_some()
    }
}

/// The coordinate system definition in effect: the one chosen for this file,
/// else longitude/latitude when the coordinates can only be that.
fn crs_def(app: &EpanetApp) -> Option<String> {
    if let Some(d) = app.prefs.basemap_crs.get(&app.file_name) {
        return Some(d.clone());
    }
    app.bounds()
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
                let b = app.bounds().unwrap_or([0.0, 0.0, 10000.0, 10000.0]);
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
        let red = Color32::from_rgb(0xc0, 0x39, 0x2b);
        let text = painter.text(
            rect.left_bottom() + vec2(8.0, -8.0),
            Align2::LEFT_BOTTOM,
            format!("Basemap: {e} (Map > Basemap > Load)"),
            FontId::proportional(12.0),
            red,
        );
        // an ✕ beside the message unloads the basemap
        let x =
            Rect::from_center_size(pos2(text.right() + 14.0, text.center().y), vec2(18.0, 18.0));
        let resp = ui
            .interact(
                x,
                egui::Id::new("basemap_error_close"),
                egui::Sense::click(),
            )
            .on_hover_text("Unload the basemap");
        if resp.hovered() {
            painter.rect_filled(x, 3.0, red.gamma_multiply(0.15));
        }
        let s = egui::Stroke::new(1.5, red);
        let d = 4.5;
        painter.line_segment([x.center() - vec2(d, d), x.center() + vec2(d, d)], s);
        painter.line_segment([x.center() + vec2(-d, d), x.center() + vec2(d, -d)], s);
        if resp.clicked() {
            unload(app);
        }
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
    let bounds = app.bounds();
    let geo = geo(app)?;
    if bounds.is_some_and(|b| !fits(geo, b)) {
        return Err("the network's coordinates are not in the chosen coordinate system".into());
    }
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
        let lonlat = app.bounds().is_some_and(looks_lonlat);
        // Longitude/latitude (EPSG:4326) unless another system was chosen for this file.
        let custom = !empty && saved.as_deref().is_some_and(|c| c != "4326");
        let saved_proj = app
            .prefs
            .basemap_crs
            .get(&app.file_name)
            .filter(|c| !c.starts_with("EPSG:"))
            .cloned();
        app.basemap.crs_edit = Some(if custom {
            saved.unwrap_or_default()
        } else if let Some(p) = saved_proj.filter(|_| !empty) {
            p
        } else if empty || lonlat {
            "4326".into()
        } else {
            // projected coordinates: the user has to say which system
            String::new()
        });
        let chosen = app.basemap.crs_edit.as_deref().unwrap_or_default();
        app.basemap.units_edit = if empty || lonlat {
            3
        } else if !chosen.is_empty() {
            crs_units(chosen)
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
    let recent = app.prefs.crs_recent.clone();
    let st = &mut app.basemap;
    let edit = st.crs_edit.as_mut().unwrap();
    let units = &mut st.units_edit;
    let selector = &mut st.selector;
    group(ui, "Internet Map Providers", &mut |ui| {
        ui.add_enabled_ui(!empty, |ui| {
            // the CRS widget: the chosen system and the recently used
            // ones in a drop-down, the full selector behind the globe button
            ui.horizontal(|ui| {
                ui.label("CRS");
                ui.label(RichText::new(" ? ").color(crate::widgets::ACCENT).strong())
                    .on_hover_text(
                        "The coordinate system the network's coordinates are in, e.g. \
                         EPSG:4326 for longitude/latitude, 2100 for the Greek Grid, 6312 for \
                         Cyprus, 3089 for Kentucky (US feet). The globe button lists every \
                         EPSG system and takes a proj string for others.",
                    );
            });
            ui.horizontal(|ui| {
                // a fixed width: the available one would widen the window every frame
                let w = 340.0;
                egui::ComboBox::from_id_salt("basemap_crs")
                    .selected_text(crs_label(edit))
                    .width(w)
                    .truncate()
                    .show_ui(ui, |ui| {
                        let mut items: Vec<String> = Vec::new();
                        for c in std::iter::once(edit.trim().to_string())
                            .chain(recent.iter().cloned())
                            .chain(["4326".to_string()])
                        {
                            if !c.is_empty() && !items.contains(&c) {
                                items.push(c);
                            }
                        }
                        for c in items {
                            if ui
                                .selectable_label(edit.trim() == c, crs_label(&c))
                                .clicked()
                            {
                                *units = crs_units(&c);
                                *edit = c;
                            }
                        }
                    });
                if globe_button(ui)
                    .on_hover_text("Select CRS (Coordinate Reference System Selector)")
                    .clicked()
                {
                    *selector = Some(CrsSelector::new(edit));
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
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
    let chosen = app
        .basemap
        .selector
        .as_mut()
        .and_then(|s| crs_selector(&ctx, s, &recent));
    if let Some(choice) = chosen {
        app.basemap.selector = None;
        if let Some(c) = choice {
            app.basemap.units_edit = crs_units(&c);
            app.basemap.crs_edit = Some(c);
            app.basemap.source_error = None;
        }
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
            let code = code.trim();
            let def = if units == 3 && code.is_empty() {
                "EPSG:4326".to_string()
            } else if code.starts_with('+') {
                // a proj string
                code.to_string()
            } else {
                format!("EPSG:{}", code.trim_start_matches("EPSG:"))
            };
            let bounds = app.bounds();
            let parsed = Geo::parse(&def).and_then(|g| match bounds {
                Some(b) if !fits(&g, b) => Err(format!(
                    "The network's coordinates ({:.0}, {:.0}) are not in {def}",
                    (b[0] + b[2]) / 2.0,
                    (b[1] + b[3]) / 2.0
                )),
                _ => Ok(g),
            });
            match parsed {
                Err(e) => {
                    app.basemap.source_error = Some(if code.trim().is_empty() {
                        "Enter the EPSG code of the network's coordinates".into()
                    } else {
                        e
                    })
                }
                Ok(_) => {
                    let key = def.strip_prefix("EPSG:").unwrap_or(&def).to_string();
                    let recent = &mut app.prefs.crs_recent;
                    recent.retain(|c| *c != key);
                    recent.insert(0, key);
                    recent.truncate(10);
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

/// How a coordinate system of the CRS field reads: "EPSG:6312 - CGRS93 /
/// Cyprus Local Transverse Mercator", or the proj string of a custom one.
fn crs_label(c: &str) -> String {
    let c = c.trim();
    if c.is_empty() {
        "Select the coordinate system…".into()
    } else if c.starts_with('+') {
        format!("Custom CRS: {c}")
    } else {
        let code = c.trim_start_matches("EPSG:");
        match code.parse().ok().and_then(epsg_name) {
            Some(name) => format!("EPSG:{code} - {name}"),
            None => format!("EPSG:{code}"),
        }
    }
}

/// The CRS Units entry of a coordinate system of the CRS field.
fn crs_units(c: &str) -> usize {
    let c = c.trim();
    if c.starts_with('+') {
        if c.contains("+proj=longlat") {
            3
        } else if c.contains("+units=us-ft") || c.contains("+units=ft") {
            1
        } else {
            2
        }
    } else {
        c.trim_start_matches("EPSG:").parse().map_or(2, epsg_units)
    }
}

/// The globe button beside the CRS field.
fn globe_button(ui: &mut Ui) -> egui::Response {
    let resp = ui.add(egui::Button::new("").min_size(vec2(30.0, 24.0)));
    let fg = ui.visuals().widgets.style(&resp).fg_stroke.color;
    crate::icons::paint(
        ui.painter(),
        crate::icons::Icon::Basemap,
        resp.rect.center(),
        16.0,
        fg,
    );
    resp
}

/// The state of the Coordinate Reference System Selector.
pub struct CrsSelector {
    filter: String,
    /// A proj string instead of a predefined system.
    custom: bool,
    proj: String,
    selected: Option<u32>,
    /// Whether the list still has to scroll to the selected system.
    scroll: bool,
    /// Whether the window has been placed in the middle of the screen.
    placed: bool,
    maximized: bool,
}

impl CrsSelector {
    fn new(current: &str) -> Self {
        let c = current.trim();
        let custom = c.starts_with('+');
        CrsSelector {
            filter: String::new(),
            custom,
            proj: if custom { c.to_string() } else { String::new() },
            selected: c.trim_start_matches("EPSG:").parse().ok(),
            scroll: true,
            placed: false,
            maximized: false,
        }
    }
}

/// A projection's name from its proj short name.
fn method_name(p: &str) -> &str {
    match p {
        "longlat" => "Geographic (longitude/latitude)",
        "tmerc" | "etmerc" => "Transverse Mercator",
        "utm" => "Universal Transverse Mercator (UTM)",
        "lcc" => "Lambert Conformal Conic",
        "merc" => "Mercator",
        "stere" | "sterea" => "Stereographic",
        "aea" => "Albers Equal Area",
        "laea" => "Lambert Azimuthal Equal Area",
        "omerc" => "Oblique Mercator",
        "cass" => "Cassini-Soldner",
        "eqc" => "Equidistant Cylindrical",
        "somerc" => "Swiss Oblique Mercator",
        "krovak" => "Krovak",
        "poly" => "Polyconic",
        _ => p,
    }
}

/// The units and method of a proj string, for the Properties box.
fn proj_properties(proj: &str) -> (String, String) {
    let param = |k: &str| proj.split_whitespace().find_map(|p| p.strip_prefix(k));
    let method = param("+proj=").unwrap_or_default();
    let units = match param("+units=") {
        Some("m") => "meters",
        Some("us-ft") => "US survey feet",
        Some("ft") => "feet",
        Some(u) => u,
        None if method == "longlat" => "degrees",
        None => "meters",
    };
    (units.to_string(), method_name(method).to_string())
}

/// Where the second column (Authority ID) of the selector tables starts.
fn crs_split(r: Rect) -> f32 {
    r.left() + r.width() * 0.68
}

/// One row of a selector table: the name, and the authority id in a second
/// column.
fn crs_row(ui: &mut Ui, selected: bool, name: &str, id: &str, stripe: bool) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::click());
    let v = ui.visuals();
    if selected {
        ui.painter().rect_filled(r, 0.0, v.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(r, 0.0, v.widgets.hovered.weak_bg_fill);
    } else if stripe {
        ui.painter().rect_filled(r, 0.0, v.faint_bg_color);
    }
    let color = if selected {
        v.selection.stroke.color
    } else {
        v.text_color()
    };
    let split = crs_split(r);
    let font = FontId::proportional(13.0);
    let clip = Rect::from_min_max(r.min, pos2(split - 6.0, r.max.y)).intersect(ui.clip_rect());
    ui.painter().with_clip_rect(clip).text(
        r.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        font.clone(),
        color,
    );
    ui.painter().text(
        pos2(split + 6.0, r.center().y),
        Align2::LEFT_CENTER,
        id,
        font,
        color,
    );
    resp.on_hover_text(name)
}

/// The header of a selector table.
fn crs_header(ui: &mut Ui) {
    let h = ui.spacing().interact_size.y;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), egui::Sense::hover());
    let v = ui.visuals();
    ui.painter()
        .rect_filled(r, 0.0, v.widgets.noninteractive.bg_fill);
    let font = FontId::proportional(13.0);
    let color = v.strong_text_color();
    ui.painter().text(
        r.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        "Coordinate Reference System",
        font.clone(),
        color,
    );
    ui.painter().text(
        pos2(crs_split(r) + 6.0, r.center().y),
        Align2::LEFT_CENTER,
        "Authority ID",
        font,
        color,
    );
}

/// A bordered box for the selector's tables.
fn crs_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(190)))
        .inner_margin(egui::Margin::same(1))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A selector table of coordinate systems; returns whether a row was
/// double-clicked.
fn crs_table(
    ui: &mut Ui,
    id: &str,
    height: f32,
    rows: &[&CrsEntry],
    selected: &mut Option<u32>,
    scroll: &mut bool,
) -> bool {
    let mut double = false;
    crs_box(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        crs_header(ui);
        let h = ui.spacing().interact_size.y;
        let mut area = egui::ScrollArea::vertical().id_salt(id);
        if std::mem::take(scroll)
            && let Some(k) = rows.iter().position(|e| Some(e.0) == *selected)
        {
            area = area.vertical_scroll_offset((k as f32 * h - height / 2.0).max(0.0));
        }
        area.min_scrolled_height(height)
            .max_height(height)
            .auto_shrink([false, false])
            .show_rows(ui, h, rows.len(), |ui, range| {
                for k in range {
                    let e = rows[k];
                    let r = crs_row(
                        ui,
                        *selected == Some(e.0),
                        &e.1,
                        &format!("EPSG:{}", e.0),
                        k % 2 == 1,
                    );
                    if r.clicked() {
                        *selected = Some(e.0);
                    }
                    if r.double_clicked() {
                        *selected = Some(e.0);
                        double = true;
                    }
                }
            });
    });
    double
}

/// The Coordinate Reference System Selector: every EPSG system (with a
/// filter), the recently used ones and the chosen one's properties, or a
/// custom proj string. Returns Some once closed: with the chosen system
/// ("6312" or a proj string) on OK, None on Cancel.
fn crs_selector(
    ctx: &egui::Context,
    sel: &mut CrsSelector,
    recent: &[String],
) -> Option<Option<String>> {
    use crate::dialogs::TITLE_H;
    let mut result = None;
    // A modal window with the dialogs' title bar: it can be dragged, resized
    // by its bottom right corner and maximized, and opens in the middle of
    // the screen. Maximized, it is another window, so the normal one keeps
    // its place and size.
    let id = egui::Id::new(("crs_selector", sel.maximized));
    let screen = ctx.content_rect();
    let center = screen.center();
    let mut area = egui::Area::new(id)
        .kind(egui::UiKind::Modal)
        .order(egui::Order::Foreground)
        .interactable(true);
    if sel.maximized {
        area = area
            .movable(false)
            .pivot(Align2::LEFT_TOP)
            .fixed_pos(screen.min + vec2(4.0, 4.0));
    } else {
        // placed by its top left corner, so that its bottom right one follows
        // the mouse when resized
        let top_left = center - vec2(590.0, 660.0 + TITLE_H) / 2.0;
        area = area
            .movable(true)
            .pivot(Align2::LEFT_TOP)
            .default_pos(top_left);
        if !std::mem::replace(&mut sel.placed, true) {
            area = area.current_pos(top_left);
        }
    }
    let style = ctx.style_of(ctx.theme());
    let fill = if style.visuals.dark_mode {
        style.visuals.panel_fill
    } else {
        Color32::from_rgb(0xf3, 0xf3, 0xf3)
    };
    let mut clicks = crate::dialogs::TitleClicks::default();
    let modal = egui::Modal::new(id)
        .area(area)
        .frame(egui::Frame::window(&style).fill(fill).inner_margin(0))
        .show(ctx, |ui| {
            let top = ui.cursor().top();
            ui.add_space(TITLE_H);
            let mut resize = egui::Resize::default()
                .id_salt("crs_resize")
                .default_size([590.0, 660.0])
                .min_size([470.0, 480.0])
                .max_size(screen.size() - vec2(8.0, 8.0 + TITLE_H));
            if sel.maximized {
                resize = resize.fixed_size(screen.size() - vec2(8.0, 8.0 + TITLE_H));
            }
            resize.show(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label("Select the coordinate system the network's coordinates are in.");
                        ui.add_space(6.0);
                        egui::ComboBox::from_id_salt("crs_kind")
                            .selected_text(if sel.custom {
                                "Custom CRS"
                            } else {
                                "Predefined CRS"
                            })
                            .width(ui.available_width() - 8.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut sel.custom, false, "Predefined CRS");
                                ui.selectable_value(&mut sel.custom, true, "Custom CRS");
                            });
                        ui.add_space(6.0);
                        let mut ok = false;
                        let mut proj = None;
                        if sel.custom {
                            ui.label(
                                "Proj string (e.g. +proj=tmerc +lat_0=0 +lon_0=24 +k=0.9996 …)",
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut sel.proj)
                                    .desired_rows(4)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace),
                            );
                            let p = sel.proj.trim();
                            match Geo::parse(p) {
                                Ok(_) => proj = Some(p.to_string()),
                                Err(e) if !p.is_empty() => {
                                    ui.label(
                                        RichText::new(e).color(Color32::from_rgb(0xc0, 0x39, 0x2b)),
                                    );
                                }
                                Err(_) => {}
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Filter");
                                ui.add(
                                    egui::TextEdit::singleline(&mut sel.filter)
                                        .hint_text(
                                            "code or name, e.g. Kentucky, 2100, UTM zone 36N",
                                        )
                                        .desired_width(f32::INFINITY),
                                );
                            });
                            let words: Vec<String> = sel
                                .filter
                                .split_whitespace()
                                .map(str::to_lowercase)
                                .collect();
                            let matches =
                                |c: &CrsEntry| words.iter().all(|w| c.2.contains(w.as_str()));
                            let all = crs_list();
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Recently Used Coordinate Reference Systems")
                                    .strong(),
                            );
                            let used: Vec<&CrsEntry> = recent
                                .iter()
                                .filter_map(|c| c.parse::<u32>().ok())
                                .filter_map(|code| all.iter().find(|e| e.0 == code))
                                .filter(|e| matches(e))
                                .collect();
                            ok |= crs_table(
                                ui,
                                "crs_recent",
                                90.0,
                                &used,
                                &mut sel.selected,
                                &mut false,
                            );
                            ui.add_space(6.0);
                            let rows: Vec<&CrsEntry> = all.iter().filter(|e| matches(e)).collect();
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Predefined Coordinate Reference Systems")
                                        .strong(),
                                );
                                ui.label(RichText::new(format!("({})", rows.len())).weak());
                            });
                            // the list takes the height the window has left
                            let h_all = (ui.available_height() - 190.0).max(120.0);
                            ok |= crs_table(
                                ui,
                                "crs_all",
                                h_all,
                                &rows,
                                &mut sel.selected,
                                &mut sel.scroll,
                            );
                            proj = sel.selected.and_then(epsg_proj);
                        }
                        // the chosen system's properties
                        ui.add_space(6.0);
                        crs_box(ui, |ui| {
                            egui::Frame::NONE
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    ui.set_min_height(86.0);
                                    ui.set_width(ui.available_width());
                                    let name = if sel.custom {
                                        Some("Custom CRS".to_string())
                                    } else {
                                        sel.selected.and_then(epsg_name)
                                    };
                                    match (&name, &proj) {
                                        (Some(name), Some(proj)) => {
                                            let (units, method) = proj_properties(proj);
                                            ui.label(RichText::new(name).strong());
                                            ui.label(format!("•  Units: {units}"));
                                            ui.label(format!("•  Method: {method}"));
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(proj.as_str())
                                                        .monospace()
                                                        .weak()
                                                        .size(11.0),
                                                )
                                                .wrap(),
                                            );
                                        }
                                        _ => {
                                            ui.label(
                                                RichText::new("No coordinate system selected")
                                                    .weak(),
                                            );
                                        }
                                    }
                                });
                        });
                        ui.add_space(10.0);
                        let chosen = if sel.custom {
                            proj.clone()
                        } else {
                            sel.selected.map(|c| c.to_string())
                        };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Help").clicked() {
                                crate::help::open(ui.ctx(), "Using a Basemap");
                            }
                            if ui.button("Cancel").clicked() {
                                result = Some(None);
                            }
                            if ui
                                .add_enabled(chosen.is_some(), egui::Button::new("OK"))
                                .clicked()
                            {
                                ok = true;
                            }
                        });
                        if ok && chosen.is_some() {
                            result = Some(chosen);
                        }
                    });
            });
            let w = ui.min_rect();
            let bar = Rect::from_min_max(pos2(w.left(), top), pos2(w.right(), top + TITLE_H));
            clicks = crate::dialogs::title_bar(
                ui,
                bar,
                "Coordinate Reference System Selector",
                fill,
                Some(sel.maximized),
                true,
            );
        });
    if clicks.maximize {
        sel.maximized = !sel.maximized;
    }
    if (modal.should_close() || clicks.close) && result.is_none() {
        result = Some(None);
    }
    result
}

/// Removes the basemap (Basemap > Unload).
fn unload(app: &mut EpanetApp) {
    app.prefs.basemap = BasemapKind::None;
    app.basemap.image = None;
    app.basemap.georef = None;
    app.basemap.align = None;
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
        unload(app);
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
            crate::help::open(&ctx, "Using a Basemap");
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
    fn kentucky_single_zone_ky10() {
        // ky10.inp (ASCE TF WDST) is in Kentucky State Plane, US feet:
        // its center should land in eastern Kentucky.
        let g = Geo::parse("EPSG:3089").unwrap();
        let b = [5709604.87, 3801262.0, 5810617.2, 3912293.0];
        assert!(fits(&g, b));
        let [lon, lat] = g.to_lonlat([5_760_000.0, 3_856_000.0]).unwrap();
        assert!(
            (-84.0..-82.0).contains(&lon) && (37.0..38.5).contains(&lat),
            "{lon} {lat}"
        );
        // the false origin (1 500 000 m, 1 000 000 m) is the projection center
        let [lon, lat] = g.to_lonlat([4_921_250.0, 3_280_833.333]).unwrap();
        assert!(
            (lon + 85.75).abs() < 1e-4 && (lat - 36.3333).abs() < 1e-3,
            "{lon} {lat}"
        );
        // not longitude/latitude
        assert!(!fits(&Geo::LonLat, b));
    }

    #[test]
    fn epsg_database() {
        // from the EPSG database: Lambert-93 (France), Kentucky North (ftUS),
        // Gauss-Krüger (Germany), and every code there with a supported projection
        for (code, lonlat) in [
            (2154, [2.35, 48.85]),
            (2246, [-84.5, 38.05]),
            (31468, [11.57, 48.14]),
        ] {
            let g = Geo::parse(&format!("EPSG:{code}")).unwrap();
            let p = g.lonlat_to(lonlat).unwrap();
            let q = g.to_lonlat(p).unwrap();
            assert!(
                (q[0] - lonlat[0]).abs() < 1e-6 && (q[1] - lonlat[1]).abs() < 1e-6,
                "{code}"
            );
        }
        assert_eq!(
            epsg_name(2246).as_deref(),
            Some("NAD83 / Kentucky North (ftUS)")
        );
        assert!(epsg_proj(60000).is_none());
        let ok = (2000..=32767)
            .filter(|&c| epsg_proj(c).is_some())
            .filter(|&c| Geo::parse(&format!("EPSG:{c}")).is_ok())
            .count();
        assert!(ok > 4000, "{ok}");
    }

    #[test]
    fn crs_list_search() {
        let list = crs_list();
        assert!(list.len() > 4000, "{}", list.len());
        let ky: Vec<u32> = list
            .iter()
            .filter(|c| c.2.contains("kentucky") && c.2.contains("single"))
            .map(|c| c.0)
            .collect();
        assert!(ky.contains(&3089), "{ky:?}");
        assert!(
            list.iter()
                .any(|c| c.0 == 2100 && c.1.contains("Greek Grid"))
        );
        assert_eq!(epsg_units(3089), 1);
        assert_eq!(epsg_units(2100), 2);
        assert_eq!(epsg_units(4326), 3);
    }

    /// The selector as drawn: its pieces of text and the modal's rectangle.
    fn draw_selector(sel: &mut CrsSelector, recent: &[String]) -> (Vec<String>, Rect) {
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            ..Default::default()
        };
        let mut texts = Vec::new();
        let mut area = Rect::NOTHING;
        for _ in 0..3 {
            let mut out = ctx.run_ui(input(), |ui| {
                crs_selector(ui.ctx(), sel, recent);
            });
            out.textures_delta.clear();
            texts.clear();
            area = Rect::NOTHING;
            for c in &out.shapes {
                if let Shape::Text(t) = &c.shape {
                    texts.push(t.galley.text().to_string());
                    area = area.union(t.visual_bounding_rect());
                }
            }
        }
        (texts, area)
    }

    #[test]
    fn crs_selector_lists_and_filters() {
        let mut sel = CrsSelector::new("6312");
        let (texts, area) = draw_selector(&mut sel, &["6312".into(), "2100".into()]);
        for t in [
            "Coordinate Reference System Selector",
            "Recently Used Coordinate Reference Systems",
            "Predefined Coordinate Reference Systems",
            "CGRS93 / Cyprus Local Transverse Mercator",
            "EPSG:2100",
            "Authority ID",
            "•  Units: meters",
            "•  Method: Transverse Mercator",
            "OK",
            "Cancel",
        ] {
            assert!(texts.iter().any(|x| x == t), "{t} missing in {texts:?}");
        }
        assert!(area.height() < 860.0, "{area:?}");
        sel.filter = "kentucky single".into();
        sel.selected = Some(3089);
        let (texts, _) = draw_selector(&mut sel, &[]);
        assert!(texts.iter().any(|x| x == "EPSG:3089"), "{texts:?}");
        assert!(
            texts.iter().any(|x| x == "•  Units: US survey feet"),
            "{texts:?}"
        );
        assert!(!texts.iter().any(|x| x == "EPSG:2100"));
        assert_eq!(
            crs_label("3089"),
            "EPSG:3089 - NAD83 / Kentucky Single Zone (ftUS)"
        );
        assert_eq!(crs_units("3089"), 1);
    }

    /// The Select a Basemap Source window keeps its width (the CRS drop-down
    /// must not widen it frame after frame).
    #[test]
    fn source_dialog_keeps_its_width() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        app.basemap.crs_edit = Some("3089".into());
        app.dialog = crate::app::Dialog::Basemap;
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            ..Default::default()
        };
        let mut widths = Vec::new();
        for _ in 0..12 {
            let mut out = ctx.run_ui(input(), |ui| crate::dialogs::show(&mut app, ui.ctx()));
            out.textures_delta.clear();
            let mut area = Rect::NOTHING;
            for c in &out.shapes {
                if let Shape::Text(t) = &c.shape
                    && t.galley.text() == "Select a Basemap Source"
                {
                    area = c.clip_rect;
                }
            }
            widths.push(area.width());
        }
        let last = *widths.last().unwrap();
        assert!(last < 560.0, "{widths:?}");
        assert_eq!(widths[5], last, "{widths:?}");
    }

    /// The ✕ beside a basemap error unloads the basemap.
    #[test]
    fn error_message_closes() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let (name, text) = eui_core::examples::ALL[0];
        app.load_example(name, text);
        app.dialog = crate::app::Dialog::None;
        // a web basemap with a coordinate system that cannot be used
        app.prefs
            .basemap_crs
            .insert(app.file_name.clone(), "EPSG:99999".into());
        app.prefs.basemap = BasemapKind::OpenStreetMap;
        app.prefs.show_basemap = true;
        let run = |app: &mut EpanetApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.refresh_geometry();
                egui::CentralPanel::default().show(ui, |ui| crate::map::show(app, ui));
            });
            out.textures_delta.clear();
            out.shapes.iter().find_map(|c| match &c.shape {
                Shape::Text(t) if t.galley.text().starts_with("Basemap:") => {
                    Some(t.visual_bounding_rect())
                }
                _ => None,
            })
        };
        run(&mut app, Vec::new());
        let msg = run(&mut app, Vec::new()).expect("no basemap message");
        let x = pos2(msg.right() + 14.0, msg.center().y);
        let click = |pressed| egui::Event::PointerButton {
            pos: x,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run(&mut app, vec![egui::Event::PointerMoved(x)]);
        run(&mut app, vec![click(true)]);
        run(&mut app, vec![click(false)]);
        assert_eq!(app.prefs.basemap, BasemapKind::None);
        assert!(run(&mut app, Vec::new()).is_none());
    }

    /// The selector resizes by its corner, maximizes with a double click on
    /// its title and closes with its ✕.
    #[test]
    fn crs_selector_resizes_maximizes_and_closes() {
        let ctx = egui::Context::default();
        let size = vec2(1400.0, 900.0);
        let mut sel = CrsSelector::new("3089");
        let run = |sel: &mut CrsSelector, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                time: Some(ctx.input(|i| i.time) + 1.0 / 60.0),
                ..Default::default()
            };
            let mut result = None;
            let mut out = ctx.run_ui(input, |ui| result = crs_selector(ui.ctx(), sel, &[]));
            out.textures_delta.clear();
            result
        };
        let rect = |maximized: bool| {
            ctx.memory(|m| m.area_rect(egui::Id::new(("crs_selector", maximized))))
                .expect("no selector")
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for _ in 0..5 {
            run(&mut sel, Vec::new());
        }
        let before = rect(false);
        assert!(
            before.width() > 580.0 && before.height() > 600.0,
            "{before:?}"
        );
        // the bottom right corner, 100 px right and 60 px down
        let from = before.right_bottom() - vec2(4.0, 4.0);
        run(&mut sel, vec![egui::Event::PointerMoved(from)]);
        run(&mut sel, vec![press(from, true)]);
        for k in 1..=10 {
            let p = from + vec2(100.0, 60.0) * (k as f32 / 10.0);
            run(&mut sel, vec![egui::Event::PointerMoved(p)]);
        }
        run(&mut sel, vec![press(from + vec2(100.0, 60.0), false)]);
        for _ in 0..3 {
            run(&mut sel, Vec::new());
        }
        let after = rect(false);
        assert!(
            (after.width() - before.width() - 100.0).abs() < 4.0,
            "{before:?} {after:?}"
        );
        assert!(
            (after.height() - before.height() - 60.0).abs() < 4.0,
            "{before:?} {after:?}"
        );
        // a double click on the title maximizes it
        let title = pos2(after.left() + 60.0, after.top() + 15.0);
        run(&mut sel, vec![egui::Event::PointerMoved(title)]);
        for pressed in [true, false, true, false] {
            run(&mut sel, vec![press(title, pressed)]);
        }
        for _ in 0..3 {
            run(&mut sel, Vec::new());
        }
        assert!(sel.maximized);
        let max = rect(true);
        assert!(max.width() > 1380.0 && max.height() > 880.0, "{max:?}");
        // the ✕ closes it
        let x = pos2(max.right() - 27.0, max.top() + 17.0);
        run(&mut sel, vec![egui::Event::PointerMoved(x)]);
        run(&mut sel, vec![press(x, true)]);
        let closed = run(&mut sel, vec![press(x, false)]);
        assert_eq!(closed, Some(None));
    }
    #[test]
    fn utm_codes() {
        assert!(epsg_proj(32634).is_some());
        assert!(epsg_proj(32799).is_none());
        assert_eq!(epsg_name(32636).as_deref(), Some("WGS 84 / UTM zone 36N"));
    }
}
