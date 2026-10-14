//! Images embedded as raw RGBA pixels (converted once from the original
//! PNG / ICO / SVG files), so no image decoder is needed at run time.

use egui::{Color32, Rect, Ui, pos2, vec2};

#[derive(Clone, Copy)]
pub struct Image {
    pub name: &'static str,
    pub size: [usize; 2],
    pub pixels: &'static [u8],
}

/// EPANET-UI's welcome photo of blue pipes (from `welcome.lfm`).
pub const WELCOME: Image = Image {
    name: "welcome",
    size: [499, 113],
    pixels: include_bytes!("../assets/welcome.rgba"),
};
/// EPANET-UI's application icon (from `epanet_ui.ico`).
pub const EPANET_UI: Image = Image {
    name: "epanet-ui",
    size: [64, 64],
    pixels: include_bytes!("../assets/epanet-ui.rgba"),
};

fn texture(ui: &Ui, img: Image) -> egui::TextureHandle {
    let id = egui::Id::new(("embedded_image", img.name));
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied(img.size, img.pixels);
    let t = ui
        .ctx()
        .load_texture(img.name, color, egui::TextureOptions::LINEAR);
    ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Shows `img` with the given height (keeping its aspect ratio).
pub fn show(ui: &mut Ui, img: Image, height: f32) -> egui::Response {
    let w = height * img.size[0] as f32 / img.size[1] as f32;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), egui::Sense::click());
    paint(ui, img, rect);
    resp
}

/// Paints `img` stretched over `rect`.
pub fn paint(ui: &Ui, img: Image, rect: Rect) {
    let t = texture(ui, img);
    ui.painter().image(
        t.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}
