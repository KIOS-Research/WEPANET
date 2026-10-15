//! EPANET-UI user interface, shared by the desktop and web apps.
//!
//! The layout follows the Lazarus EPANET-UI: menu bar and toolbar on top,
//! Map Viewer on the left, map or report in the middle, Project Explorer with
//! the Property Editor on the right, and the status bar at the bottom.

mod app;
mod basemap;
mod calibration;
mod catalog;
mod data_view;
mod dialogs;
mod explorer;
mod export;
mod fireflow;
mod fonts;
mod forms;
mod geometry;
mod help;
mod icons;
mod images;
mod import;
mod import_shp;
mod io;
mod map;
mod menu;
mod msx;
mod plugin_manager;
mod plugins;
mod reports;
mod scripting;
mod setup;
mod shapefile;
mod source;
mod status;
mod style;
mod viewer;
mod widgets;

pub use app::EpanetApp;

/// Application name shown in title bars.
pub const APP_NAME: &str = "WEPANET";

/// The EPANET-UI icon (a water drop, from the Lazarus app) for window title bars.
pub fn app_icon() -> egui::IconData {
    let img = images::EPANET_UI;
    egui::IconData {
        rgba: img.pixels.to_vec(),
        width: img.size[0] as u32,
        height: img.size[1] as u32,
    }
}
