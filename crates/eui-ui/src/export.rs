//! Map > Export: the map as drawn, legends included, as a PNG image on the
//! clipboard or in a file (EPANET-UI's Export > To Clipboard / To File); and
//! the same for a report showing a graph (its Copy to Clipboard / Save to
//! File, which EPANET-UI does with the chart's picture).
//!
//! egui is asked for a screenshot of the window, which arrives a frame later;
//! it is then cut down to the map or the report.

use crate::app::{EpanetApp, ViewMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTarget {
    Clipboard,
    File,
}

/// What is exported.
#[derive(Clone, Debug, Default, PartialEq)]
enum Subject {
    #[default]
    Map,
    /// A report's graph, with the file name to suggest.
    Report(String),
}

#[derive(Default)]
pub struct ExportState {
    /// The map's area in the window, from the last frame it was drawn.
    pub map_rect: Option<egui::Rect>,
    /// The report's area below its title bar, from the last frame it was drawn.
    pub report_rect: Option<egui::Rect>,
    subject: Subject,
    pending: Option<ExportTarget>,
    /// An SVG file is to be written from the map's next drawing.
    pub svg: bool,
    /// Frames to wait (for the map to be shown) before taking the screenshot.
    wait: u8,
    requested: bool,
}

impl ExportState {
    /// Whether a picture is being taken (controls drawn on it are left out).
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

/// Starts an export of the map.
pub fn request(app: &mut EpanetApp, target: ExportTarget) {
    let ex = &mut app.export;
    ex.subject = Subject::Map;
    ex.pending = Some(target);
    ex.requested = false;
    // let the map be drawn (and any menu close) before the screenshot
    ex.wait = if app.view_mode == ViewMode::Map { 1 } else { 2 };
    app.view_mode = ViewMode::Map;
    app.status = "Exporting the map…".into();
}

/// Starts a copy (or save) of the graph the report shows, as a picture;
/// `name` is the file name to suggest, without its extension.
pub fn request_report(app: &mut EpanetApp, target: ExportTarget, name: &str) {
    let ex = &mut app.export;
    ex.subject = Subject::Report(name.to_string());
    ex.pending = Some(target);
    ex.requested = false;
    // let the report's menu close before the screenshot
    ex.wait = 2;
}

/// Takes the screenshot when the map is ready and handles it when it arrives.
pub fn poll(app: &mut EpanetApp, ctx: &egui::Context) {
    let Some(target) = app.export.pending else {
        return;
    };
    if !app.export.requested {
        if app.export.wait > 0 {
            app.export.wait -= 1;
            ctx.request_repaint();
            return;
        }
        app.export.requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        ctx.request_repaint();
        return;
    }
    let shot = ctx.input(|i| {
        i.raw.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    let Some(image) = shot else {
        ctx.request_repaint();
        return;
    };
    app.export.pending = None;
    let (rect, stem, map) = match &app.export.subject {
        Subject::Map => (app.export.map_rect, app.file_name.clone(), true),
        Subject::Report(name) => (app.export.report_rect, name.clone(), false),
    };
    let Some(rect) = rect else {
        app.status = if map {
            "The map is not shown; nothing to export"
        } else {
            "The report is not shown; nothing to copy"
        }
        .into();
        return;
    };
    let picture = image.region(&rect, Some(ctx.pixels_per_point()));
    use crate::i18n::{tr, trf};
    match target {
        ExportTarget::Clipboard => {
            ctx.copy_image(picture);
            app.status = tr(if map {
                "Map copied to the clipboard"
            } else {
                "Graph copied to the clipboard"
            })
            .into();
        }
        ExportTarget::File => match png(&picture) {
            Ok(bytes) => {
                let name = format!("{stem}.png");
                app.status =
                    match crate::io::save_bytes(&bytes, &name, "PNG image", "png", "image/png") {
                        Ok(()) => trf("Saved {}", &[&name]),
                        Err(e) => format!("Could not save the image: {e}"),
                    };
            }
            Err(e) => app.status = format!("Could not make the image: {e}"),
        },
    }
}

/// Starts Export > To SVG File: the map is drawn once more and what it
/// draws is written as SVG (see [`take_svg`]).
pub fn request_svg(app: &mut EpanetApp) {
    app.export.svg = true;
    app.view_mode = ViewMode::Map;
    app.status = "Exporting the map…".into();
}

/// Called at the end of the map's drawing: when an SVG export was asked
/// for, the shapes drawn on the map since `first` are written as SVG and
/// saved.
pub fn take_svg(app: &mut EpanetApp, ui: &egui::Ui, rect: egui::Rect, first: usize) {
    if !app.export.svg {
        return;
    }
    app.export.svg = false;
    let shapes = map_shapes(ui, first);
    let svg = to_svg(&shapes, rect);
    let name = format!("{}.svg", app.file_name);
    use crate::i18n::trf;
    app.status =
        match crate::io::save_bytes(svg.as_bytes(), &name, "SVG image", "svg", "image/svg+xml") {
            Ok(()) => trf("Saved {}", &[&name]),
            Err(e) => format!("Could not save the image: {e}"),
        };
}

/// The shapes drawn on `ui`'s layer from the `first` on.
fn map_shapes(ui: &egui::Ui, first: usize) -> Vec<egui::epaint::ClippedShape> {
    let layer = ui.layer_id();
    ui.ctx().graphics_mut(|g| {
        let Some(list) = g.get_mut(layer) else {
            return Vec::new();
        };
        let end = list.next_idx().0;
        let mut out = Vec::with_capacity(end.saturating_sub(first));
        for k in first..end {
            list.mutate_shape(egui::layers::ShapeIdx(k), |c| out.push(c.clone()));
        }
        out
    })
}

/// An SVG color attribute, e.g. `fill="#ff0000"`, with its opacity when
/// see-through; `none` for transparent.
fn paint(attr: &str, c: egui::Color32) -> String {
    if c.a() == 0 {
        return format!(r#"{attr}="none""#);
    }
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let mut s = format!(r##"{attr}="#{r:02x}{g:02x}{b:02x}""##);
    if a < 255 {
        s += &format!(r#" {attr}-opacity="{:.3}""#, a as f32 / 255.0);
    }
    s
}

fn stroke_attrs(width: f32, color: egui::Color32) -> String {
    if width <= 0.0 || color.a() == 0 {
        r#"stroke="none""#.into()
    } else {
        format!(r#"{} stroke-width="{width:.2}""#, paint("stroke", color))
    }
}

fn solid(c: &egui::epaint::ColorMode) -> egui::Color32 {
    match c {
        egui::epaint::ColorMode::Solid(c) => *c,
        _ => egui::Color32::TRANSPARENT,
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One shape (and those inside it) as SVG elements, moved by `-o`.
fn svg_shape(out: &mut String, shape: &egui::Shape, o: egui::Vec2) {
    use egui::Shape;
    use std::fmt::Write as _;
    let p = |q: egui::Pos2| {
        let q = q - o;
        format!("{:.2},{:.2}", q.x, q.y)
    };
    match shape {
        Shape::Vec(v) => {
            for s in v {
                svg_shape(out, s, o);
            }
        }
        Shape::Circle(c) => {
            let at = c.center - o;
            let _ = writeln!(
                out,
                r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" {} {}/>"#,
                at.x,
                at.y,
                c.radius,
                paint("fill", c.fill),
                stroke_attrs(c.stroke.width, c.stroke.color)
            );
        }
        Shape::Ellipse(e) => {
            let at = e.center - o;
            let _ = writeln!(
                out,
                r#"<ellipse cx="{:.2}" cy="{:.2}" rx="{:.2}" ry="{:.2}" {} {}/>"#,
                at.x,
                at.y,
                e.radius.x,
                e.radius.y,
                paint("fill", e.fill),
                stroke_attrs(e.stroke.width, e.stroke.color)
            );
        }
        Shape::LineSegment { points, stroke } => {
            let _ = writeln!(
                out,
                r#"<polyline points="{} {}" fill="none" {}/>"#,
                p(points[0]),
                p(points[1]),
                stroke_attrs(stroke.width, stroke.color)
            );
        }
        Shape::Path(path) => {
            if path.points.len() < 2 {
                return;
            }
            let pts: Vec<String> = path.points.iter().map(|&q| p(q)).collect();
            let (tag, fill) = if path.closed {
                ("polygon", paint("fill", path.fill))
            } else {
                ("polyline", r#"fill="none""#.to_string())
            };
            let _ = writeln!(
                out,
                r#"<{tag} points="{}" {fill} {}/>"#,
                pts.join(" "),
                stroke_attrs(path.stroke.width, solid(&path.stroke.color))
            );
        }
        Shape::Rect(r) => {
            let min = r.rect.min - o;
            let _ = writeln!(
                out,
                r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="{}" {} {}/>"#,
                min.x,
                min.y,
                r.rect.width(),
                r.rect.height(),
                r.corner_radius.nw,
                paint("fill", r.fill),
                stroke_attrs(r.stroke.width, r.stroke.color)
            );
        }
        Shape::Text(t) => {
            let job = &t.galley.job;
            let first = job.sections.first();
            let size = first.map_or(12.5, |s| s.format.font_id.size);
            let color = t
                .override_text_color
                .or(first.map(|s| s.format.color))
                .filter(|&c| c != egui::Color32::PLACEHOLDER)
                .unwrap_or(t.fallback_color);
            for placed in &t.galley.rows {
                let text = placed.row.text();
                let Some(g) = placed.row.glyphs.first() else {
                    continue;
                };
                if text.trim().is_empty() {
                    continue;
                }
                // glyph positions are at the baseline
                let at = t.pos + placed.pos.to_vec2() + g.pos.to_vec2() - o;
                let _ = writeln!(
                    out,
                    r#"<text x="{:.2}" y="{:.2}" font-size="{size:.1}" {} xml:space="preserve">{}</text>"#,
                    at.x,
                    at.y,
                    paint("fill", color),
                    escape(&text)
                );
            }
        }
        Shape::QuadraticBezier(b) => {
            let [a, c, d] = b.points;
            let _ = writeln!(
                out,
                r#"<path d="M{} Q{} {}" fill="none" {}/>"#,
                p(a),
                p(c),
                p(d),
                stroke_attrs(b.stroke.width, solid(&b.stroke.color))
            );
        }
        Shape::CubicBezier(b) => {
            let [a, c1, c2, d] = b.points;
            let _ = writeln!(
                out,
                r#"<path d="M{} C{} {} {}" fill="none" {}/>"#,
                p(a),
                p(c1),
                p(c2),
                p(d),
                stroke_attrs(b.stroke.width, solid(&b.stroke.color))
            );
        }
        // images (basemaps, map layers) and callbacks are left out
        _ => {}
    }
}

/// Writes shapes as an SVG picture of `rect`. Images (basemap tiles, map
/// layers) are left out; everything else stays vectors and text.
pub(crate) fn to_svg(shapes: &[egui::epaint::ClippedShape], rect: egui::Rect) -> String {
    let (w, h) = (rect.width(), rect.height());
    let mut out = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.2} {h:.2}" font-family="Segoe UI, Selawik, Helvetica, Arial, sans-serif">
<defs><clipPath id="map"><rect width="{w:.2}" height="{h:.2}"/></clipPath></defs>
<g clip-path="url(#map)" stroke-linecap="round" stroke-linejoin="round">
"#
    );
    let o = rect.min.to_vec2();
    for c in shapes {
        if c.clip_rect.intersects(rect) {
            svg_shape(&mut out, &c.shape, o);
        }
    }
    out += "</g>\n</svg>\n";
    out
}

/// Encodes an image as PNG.
fn png(img: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = img.size;
    let rgba: Vec<u8> = img
        .pixels
        .iter()
        .flat_map(|c| c.to_srgba_unmultiplied())
        .collect();
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("bad image size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A report's graph goes to the clipboard as the picture of the report's
    /// area, cut from the window's screenshot.
    #[test]
    fn report_graph_is_copied_as_a_picture() {
        let (ctx, mut app) = crate::msx::tests::new_app();
        let area = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(300.0, 200.0));
        app.export.report_rect = Some(area);
        request_report(&mut app, ExportTarget::Clipboard, "System_Flow_Report");
        let shot = egui::ColorImage::new([800, 600], vec![egui::Color32::WHITE; 800 * 600]);
        let mut copied = None;
        for k in 0..5 {
            let mut events = Vec::new();
            if k == 4 {
                events.push(egui::Event::Screenshot {
                    viewport_id: egui::ViewportId::ROOT,
                    user_data: egui::UserData::default(),
                    image: std::sync::Arc::new(shot.clone()),
                });
            }
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| poll(&mut app, ui.ctx()),
            );
            out.textures_delta.clear();
            for c in out.platform_output.commands {
                if let egui::OutputCommand::CopyImage(img) = c {
                    copied = Some(img.size);
                }
            }
        }
        assert_eq!(copied, Some([300, 200]));
        assert_eq!(app.status, "Graph copied to the clipboard");
        assert!(app.export.pending.is_none());
    }

    #[test]
    fn png_round_trip() {
        let img = egui::ColorImage::new([3, 2], vec![egui::Color32::from_rgb(10, 20, 30); 6]);
        let bytes = super::png(&img).unwrap();
        let back = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!((back.width(), back.height()), (3, 2));
        assert_eq!(back.get_pixel(1, 1).0, [10, 20, 30, 255]);
    }
}
