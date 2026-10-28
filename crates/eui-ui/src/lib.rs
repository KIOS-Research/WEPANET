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
mod dock;
mod dxf;
mod explorer;
mod export;
mod filters;
mod fireflow;
mod fonts;
mod forms;
mod geometry;
mod group;
mod help;
mod i18n;
mod icons;
mod images;
mod import;
mod import_csv;
mod import_dxf;
mod import_shp;
mod io;
mod layers;
mod locater;
mod map;
mod menu;
mod msx;
mod multi;
mod plugin_manager;
mod plugin_meta;
mod plugins;
mod project;
mod python;
#[cfg(target_arch = "wasm32")]
mod recent;
mod reports;
mod scripting;
mod setup;
mod shapefile;
mod source;
mod status;
mod style;
#[cfg(test)]
mod test_plugins;
mod toolbars;
mod viewer;
mod widgets;
mod wizard;

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
