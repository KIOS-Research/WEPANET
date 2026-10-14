//! WEPANET desktop app. Usage: `epanet-ui [network.inp]`
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    env_logger::init();
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(eui_ui::APP_NAME)
            .with_inner_size([1280.0, 760.0])
            .with_maximized(true)
            .with_min_inner_size([800.0, 520.0])
            .with_drag_and_drop(true)
            .with_icon(std::sync::Arc::new(eui_ui::app_icon())),
        ..Default::default()
    };
    eframe::run_native(
        eui_ui::APP_NAME,
        options,
        Box::new(move |cc| {
            let mut app = eui_ui::EpanetApp::new(cc);
            match file {
                Some(path) => app.open_path(path),
                None => app.open_last_file(),
            }
            Ok(Box::new(app))
        }),
    )
}
