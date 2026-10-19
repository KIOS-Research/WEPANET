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
    /// Frames to wait (for the map to be shown) before taking the screenshot.
    wait: u8,
    requested: bool,
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
