//! Map > Export: the map as drawn, legends included, as a PNG image on the
//! clipboard or in a file (EPANET-UI's Export > To Clipboard / To File).
//!
//! egui is asked for a screenshot of the window, which arrives a frame later;
//! it is then cut down to the map area.

use crate::app::{EpanetApp, ViewMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTarget {
    Clipboard,
    File,
}

#[derive(Default)]
pub struct ExportState {
    /// The map's area in the window, from the last frame it was drawn.
    pub map_rect: Option<egui::Rect>,
    pending: Option<ExportTarget>,
    /// Frames to wait (for the map to be shown) before taking the screenshot.
    wait: u8,
    requested: bool,
}

/// Starts an export of the map.
pub fn request(app: &mut EpanetApp, target: ExportTarget) {
    let ex = &mut app.export;
    ex.pending = Some(target);
    ex.requested = false;
    // let the map be drawn (and any menu close) before the screenshot
    ex.wait = if app.view_mode == ViewMode::Map { 1 } else { 2 };
    app.view_mode = ViewMode::Map;
    app.status = "Exporting the map…".into();
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
    let Some(rect) = app.export.map_rect else {
        app.status = "The map is not shown; nothing to export".into();
        return;
    };
    let map = image.region(&rect, Some(ctx.pixels_per_point()));
    match target {
        ExportTarget::Clipboard => {
            ctx.copy_image(map);
            app.status = "Map copied to the clipboard".into();
        }
        ExportTarget::File => match png(&map) {
            Ok(bytes) => {
                let name = format!("{}.png", app.file_name);
                app.status =
                    match crate::io::save_bytes(&bytes, &name, "PNG image", "png", "image/png") {
                        Ok(()) => format!("Map saved as {name}"),
                        Err(e) => format!("Could not save the map: {e}"),
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
    #[test]
    fn png_round_trip() {
        let img = egui::ColorImage::new([3, 2], vec![egui::Color32::from_rgb(10, 20, 30); 6]);
        let bytes = super::png(&img).unwrap();
        let back = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!((back.width(), back.height()), (3, 2));
        assert_eq!(back.get_pixel(1, 1).0, [10, 20, 30, 255]);
    }
}
