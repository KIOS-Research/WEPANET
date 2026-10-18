//! Toolbar and menu icons. They are EPANET-UI's own images (icons8,
//! taken from the image lists in `main.lfm`), used as masks so that they can
//! be drawn in any color: the theme's text color, or gray when disabled.
//! The few icons EPANET-UI does not have are drawn with the painter.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    // file menu and speed bar
    New,
    Open,
    Save,
    SaveAs,
    Import,
    Preferences,
    #[cfg(not(target_arch = "wasm32"))]
    Quit,
    Back,
    ZoomIn,
    ZoomOut,
    FullExtent,
    Analyze,
    Report,
    Help,
    // edit
    Copy,
    Paste,
    Reverse,
    Reshape,
    Undo,
    Redo,
    GroupEdit,
    GroupDelete,
    Convert,
    // map
    Basemap,
    Export,
    Query,
    Settings,
    // project
    Setup,
    Add,
    Delete,
    Locate,
    Summary,
    Details,
    // help
    Units,
    ErrorCodes,
    Tutorial,
    About,
    // object types
    Junction,
    Reservoir,
    Tank,
    Pipe,
    Pump,
    Valve,
    Label,
    // reports
    StatusReport,
    Energy,
    Pumping,
    SystemFlow,
    TimeSeries,
    Percentiles,
    Profile,
    NodeTable,
    LinkTable,
    Calibration,
    FireFlow,
    // misc
    Pencil,
    /// The Python Console: the Python logo, in its own colors.
    Python,
    Layers,
    Menu,
}

/// An image list taken from EPANET-UI: square BGRA images stored one after another.
struct Sheet {
    name: &'static str,
    size: usize,
    pixels: &'static [u8],
}

/// `MaterialImageList`: the toolbar and file menu images (48 × 48).
const TOOLBAR: Sheet = Sheet {
    name: "toolbar",
    size: 48,
    pixels: include_bytes!("../assets/icons/toolbar-48.bgra"),
};
/// `PopupImageList`: the drop-down menu images (24 × 24).
const MENU: Sheet = Sheet {
    name: "menu",
    size: 24,
    pixels: include_bytes!("../assets/icons/menu-24.bgra"),
};

/// `ImportImageList`: the File > Import images, in color (32 × 32): GIS
/// shapefile, CAD DXF file and CSV text file.
const IMPORT: Sheet = Sheet {
    name: "import",
    size: 32,
    pixels: include_bytes!("../assets/icons/import-32.bgra"),
};

/// Draws File > Import image `index` (0 SHP, 1 DXF, 2 CSV) in its own colors.
pub fn paint_import(p: &Painter, index: usize, c: Pos2, size: f32) {
    let id = egui::Id::new(("icon", IMPORT.name, index));
    let ctx = p.ctx();
    let t = ctx
        .data(|d| d.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let n = IMPORT.size * IMPORT.size * 4;
            let rgba: Vec<u8> = IMPORT.pixels[index * n..(index + 1) * n]
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|px| [px[2], px[1], px[0], px[3]])
                .collect();
            let img = egui::ColorImage::from_rgba_unmultiplied([IMPORT.size, IMPORT.size], &rgba);
            let t = ctx.load_texture(
                format!("icon-import-{index}"),
                img,
                egui::TextureOptions::LINEAR,
            );
            ctx.data_mut(|d| d.insert_temp(id, t.clone()));
            t
        });
    p.image(
        t.id(),
        Rect::from_center_size(c, vec2(size, size)),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// Where EPANET-UI keeps the image of an icon, if it has one.
fn image_of(icon: Icon) -> Option<(&'static Sheet, usize)> {
    let t = |i| Some((&TOOLBAR, i));
    let m = |i| Some((&MENU, i));
    match icon {
        Icon::Back => t(0),
        Icon::Copy => t(1),
        Icon::Paste => t(2),
        Icon::Reshape => t(3),
        Icon::Reverse => t(4),
        Icon::GroupEdit => t(5),
        Icon::Basemap => t(8),
        Icon::Export => t(9),
        Icon::Query => t(10),
        Icon::ZoomIn => t(11),
        Icon::ZoomOut => t(12),
        Icon::FullExtent => t(13),
        Icon::Settings => t(14),
        Icon::Setup => t(15),
        Icon::Add => t(16),
        Icon::Delete => t(17),
        Icon::Locate => t(18),
        Icon::Summary => t(19),
        Icon::Details => t(20),
        Icon::Analyze => t(21),
        Icon::Report => t(22),
        Icon::Help => t(23),
        Icon::Units => t(24),
        Icon::ErrorCodes => t(25),
        Icon::Tutorial => t(26),
        Icon::About => t(27),
        Icon::New => t(28),
        Icon::Open => t(29),
        Icon::Save => t(30),
        Icon::GroupDelete => t(31),
        Icon::Convert => t(32),
        Icon::SaveAs => t(34),
        Icon::Import => t(36),
        #[cfg(not(target_arch = "wasm32"))]
        Icon::Quit => t(37),
        Icon::Preferences => t(38),
        Icon::Junction | Icon::NodeTable => m(0),
        Icon::Reservoir => m(1),
        Icon::Tank => m(2),
        Icon::Pipe | Icon::LinkTable => m(3),
        Icon::Pump => m(4),
        Icon::Valve => m(5),
        Icon::Label => m(6),
        Icon::StatusReport => m(7),
        Icon::TimeSeries => m(10),
        Icon::Profile => m(12),
        Icon::Energy => m(13),
        Icon::SystemFlow => m(14),
        Icon::Calibration => m(15),
        Icon::Pumping => m(16),
        Icon::Menu => m(35),
        Icon::Percentiles => m(36),
        Icon::FireFlow => m(40),
        Icon::Undo | Icon::Redo | Icon::Pencil | Icon::Python | Icon::Layers => None,
    }
}

/// The image as a white mask whose opacity is the darkness of the original,
/// so that tinting it gives an icon of any color.
fn texture(p: &Painter, sheet: &Sheet, index: usize) -> egui::TextureHandle {
    let id = egui::Id::new(("icon", sheet.name, index));
    let ctx = p.ctx();
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let n = sheet.size * sheet.size * 4;
    let bgra = &sheet.pixels[index * n..(index + 1) * n];
    let mask: Vec<u8> = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| {
            let lum = (px[0] as u32 + px[1] as u32 + px[2] as u32) as f32 / (3.0 * 255.0);
            let alpha = (px[3] as f32 * (1.0 - lum)).round() as u8;
            [255, 255, 255, alpha]
        })
        .collect();
    let img = egui::ColorImage::from_rgba_unmultiplied([sheet.size, sheet.size], &mask);
    let t = ctx.load_texture(
        format!("icon-{}-{index}", sheet.name),
        img,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Draws `icon` centered at `c`, `size` pixels across, in color `fg`.
pub fn paint(p: &Painter, icon: Icon, c: Pos2, size: f32, fg: Color32) {
    if let Some((sheet, index)) = image_of(icon) {
        let t = texture(p, sheet, index);
        p.image(
            t.id(),
            Rect::from_center_size(c, vec2(size, size)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            fg,
        );
        return;
    }
    let r = size * 0.5;
    let s = Stroke::new((size / 12.0).clamp(1.2, 2.2), fg);
    // Points in a unit box: (-1,-1) top-left .. (1,1) bottom-right.
    let at = |x: f32, y: f32| c + vec2(x * r, y * r);
    match icon {
        Icon::Undo | Icon::Redo => {
            let flip = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let pi = std::f32::consts::PI;
            let pts: Vec<Pos2> = (0..=20)
                .map(|k| {
                    let a = -0.6 * pi + 1.45 * pi * k as f32 / 20.0;
                    at(flip * 0.6 * a.cos(), 0.1 + 0.6 * a.sin())
                })
                .collect();
            let start = pts[0];
            p.add(Shape::line(pts, s));
            // Arrow head at the start of the arc.
            let tip = start + vec2(-flip * 0.25 * r, -0.05 * r);
            let back = vec2(flip, -0.2).normalized() * 0.32 * r;
            let side = vec2(-back.y, back.x) * 0.6;
            p.add(Shape::convex_polygon(
                vec![tip, tip + back + side, tip + back - side],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Python => {
            // two snakes, blue at the top left and yellow (the blue turned
            // half a circle) at the bottom right, each a head with an eye,
            // a bar across and a tail down the side
            let blue = Color32::from_rgb(0x37, 0x76, 0xab);
            let yellow = Color32::from_rgb(0xff, 0xd4, 0x3b);
            for (flip, color) in [(1.0, blue), (-1.0, yellow)] {
                let block = |x0: f32, y0: f32, x1: f32, y1: f32, round: f32| {
                    let rect =
                        Rect::from_two_pos(at(flip * x0, flip * y0), at(flip * x1, flip * y1));
                    p.rect_filled(rect, round * r, color);
                };
                block(-0.5, -0.98, 0.5, -0.3, 0.22); // head
                block(-0.98, -0.47, 0.45, -0.05, 0.14); // bar
                block(-0.98, -0.47, -0.56, 0.45, 0.2); // tail
                p.circle_filled(at(flip * -0.24, flip * -0.72), 0.1 * r, Color32::WHITE);
            }
        }
        Icon::Layers => {
            // sheets one above the other: the top one whole, the edges of
            // two below
            let top = [(0.0, -0.75), (0.95, -0.3), (0.0, 0.15), (-0.95, -0.3)];
            p.add(Shape::convex_polygon(
                top.iter().map(|&(x, y)| at(x, y)).collect(),
                fg.gamma_multiply(0.25),
                s,
            ));
            for y in [0.15, 0.5] {
                p.add(Shape::line(
                    vec![at(-0.95, y), at(0.0, y + 0.45), at(0.95, y)],
                    s,
                ));
            }
        }
        _ => {
            // Pencil.
            let pts = [
                (-0.75, 0.75),
                (-0.7, 0.4),
                (0.4, -0.7),
                (0.7, -0.4),
                (-0.4, 0.7),
            ];
            p.add(Shape::closed_line(
                pts.iter().map(|&(x, y)| at(x, y)).collect(),
                s,
            ));
            p.line_segment([at(0.2, -0.5), at(0.5, -0.2)], s);
        }
    }
}

pub fn menu_item(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    item(ui, label, enabled, |p, c, fg| paint(p, icon, c, 20.0, fg))
}

/// A menu item with a check mark where the icon goes when `checked`
/// (EPANET-UI's checkable menu items).
pub fn check_item(ui: &mut egui::Ui, label: &str, checked: bool) -> egui::Response {
    item(ui, label, true, |p, c, fg| {
        if checked {
            let s = egui::Stroke::new(1.4, fg);
            p.line_segment([c + vec2(-5.0, 0.0), c + vec2(-1.5, 3.5)], s);
            p.line_segment([c + vec2(-1.5, 3.5), c + vec2(5.0, -4.0)], s);
        }
    })
}

fn item(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    mark: impl FnOnce(&egui::Painter, egui::Pos2, Color32),
) -> egui::Response {
    let label = crate::i18n::tr(label);
    let font = egui::FontId::proportional(13.0);
    let text_w = ui.fonts_mut(|f| {
        f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
    });
    let width = (text_w + 52.0).max(ui.available_width()).max(170.0);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, 30.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let v = ui.visuals();
    let fg = if enabled {
        v.text_color()
    } else {
        v.weak_text_color().gamma_multiply(0.6)
    };
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, v.widgets.hovered.weak_bg_fill);
    }
    mark(ui.painter(), rect.left_center() + vec2(16.0, 0.0), fg);
    ui.painter().text(
        rect.left_center() + vec2(34.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        fg,
    );
    if enabled {
        resp
    } else {
        resp.on_hover_text("Not available yet")
    }
}
