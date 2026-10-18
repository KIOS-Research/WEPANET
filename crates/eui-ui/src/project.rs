//! WEPANET projects (`.wepanet`): the network with what goes with it on the
//! map — its coordinate reference system, basemap, map layers, themes and the
//! view — in one ZIP file: `network.inp` (an EPANET input file like any
//! other), the network's EPANET-MSX file `network.msx` when it has one, the
//! image basemap `basemap.png` when there is one, and `project.json`.

use std::io::{Read as _, Write as _};

use serde::{Deserialize, Serialize};

use crate::app::EpanetApp;
use crate::basemap::BasemapKind;
use crate::layers::MapLayer;
use crate::style::{LinkTheme, NodeTheme};

pub const EXT: &str = "wepanet";
const INP: &str = "network.inp";
const MSX: &str = "network.msx";
const IMAGE: &str = "basemap.png";
const JSON: &str = "project.json";

/// Whether `name` is a project file's.
pub fn is_project(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".wepanet")
}

/// What a project keeps besides the network.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub crs: Option<String>,
    pub basemap: BasemapKind,
    pub show_basemap: bool,
    pub basemap_fade: f32,
    pub basemap_gray: bool,
    /// The image basemap's name and where it lies (its picture is `basemap.png`).
    pub basemap_image: Option<(String, [f64; 4])>,
    pub map_layers: Vec<MapLayer>,
    pub node_theme: Option<NodeTheme>,
    pub link_theme: Option<LinkTheme>,
    /// The map's center and scale.
    pub view: Option<([f64; 2], f64)>,
}

/// A project file read: the network's text, its MSX file, the image
/// basemap's picture and the rest.
pub struct Opened {
    pub inp: String,
    pub msx: Option<String>,
    pub image: Option<Vec<u8>>,
    pub project: Project,
}

/// The project file of what is open.
pub fn write(app: &EpanetApp) -> Result<Vec<u8>, String> {
    let image = crate::basemap::image_for_project(app);
    let project = Project {
        version: 1,
        crs: crate::basemap::crs_def(app),
        basemap: app.prefs.basemap,
        show_basemap: app.prefs.show_basemap,
        basemap_fade: app.prefs.basemap_fade,
        basemap_gray: app.prefs.basemap_gray,
        basemap_image: image.as_ref().map(|(name, _, b)| (name.clone(), *b)),
        map_layers: app.prefs.map_layers.clone(),
        node_theme: Some(app.prefs.node_theme),
        link_theme: Some(app.prefs.link_theme),
        view: Some((app.view.center, app.view.scale)),
    };
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut buf);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        z.start_file(name, opts).map_err(|e| e.to_string())?;
        z.write_all(bytes).map_err(|e| e.to_string())
    };
    put(INP, app.net.to_inp().as_bytes())?;
    if let Some(m) = &app.net.msx {
        put(MSX, m.text.as_bytes())?;
    }
    if let Some((_, png, _)) = &image {
        put(IMAGE, png)?;
    }
    put(JSON, json.as_bytes())?;
    z.finish().map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Reads a project file.
pub fn read(bytes: &[u8]) -> Result<Opened, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "Not a WEPANET project file".to_string())?;
    let mut file = |name: &str| -> Option<Vec<u8>> {
        let mut f = zip.by_name(name).ok()?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).ok()?;
        Some(out)
    };
    let inp = file(INP).ok_or("The project file holds no network (network.inp)")?;
    let project = match file(JSON) {
        Some(j) => serde_json::from_slice(&j)
            .map_err(|e| format!("The project file's project.json cannot be read: {e}"))?,
        None => Project::default(),
    };
    Ok(Opened {
        inp: crate::io::decode_text(&inp),
        msx: file(MSX).map(|m| crate::io::decode_text(&m)),
        image: file(IMAGE),
        project,
    })
}

/// Opens the project file `name`: its network (asking first if the open one
/// has unsaved changes), then what goes with it, once the network is in.
pub fn open(app: &mut EpanetApp, name: &str, bytes: &[u8], path: Option<std::path::PathBuf>) {
    match read(bytes) {
        Ok(o) => {
            let msx_name = name
                .trim_end_matches(".wepanet")
                .trim_end_matches(".WEPANET")
                .to_string()
                + ".msx";
            app.msx.pending = o
                .msx
                .as_deref()
                .map(|t| eui_core::MsxFile::new(&msx_name, t));
            app.pending_project = Some((o.project, o.image));
            app.load_document(name, &o.inp, path);
        }
        Err(e) => app.status = format!("Could not open {name}: {e}"),
    }
}

/// Puts back what a project keeps besides its network (the network is in).
pub fn apply(app: &mut EpanetApp, project: Project, image: Option<Vec<u8>>) {
    if let Some(crs) = project.crs {
        app.set_project_crs(crs);
        app.dirty = false;
    }
    app.prefs.basemap = project.basemap;
    app.prefs.show_basemap = project.show_basemap;
    app.prefs.basemap_fade = project.basemap_fade;
    app.prefs.basemap_gray = project.basemap_gray;
    if let (Some((name, bounds)), Some(png)) = (project.basemap_image, image) {
        let ctx = app.ctx.clone();
        if let Err(e) = crate::basemap::restore_image(app, &ctx, &name, &png, bounds) {
            app.status = format!("The project's basemap image cannot be read: {e}");
        }
    } else if app.prefs.basemap == BasemapKind::Image {
        app.prefs.basemap = BasemapKind::None;
    }
    app.saved_layers = project.map_layers.clone();
    app.prefs.map_layers = project.map_layers;
    if let Some(t) = project.node_theme {
        app.prefs.node_theme = t;
    }
    if let Some(t) = project.link_theme {
        app.prefs.link_theme = t;
    }
    if let Some((center, scale)) = project.view
        && scale.is_finite()
        && scale > 0.0
    {
        app.view.place(center, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_goes_out_and_comes_back() {
        let ctx = egui::Context::default();
        let mut a = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        a.load_example("Net1", eui_core::examples::NET1);
        a.set_project_crs("EPSG:2100".into());
        a.prefs.basemap = BasemapKind::OpenStreetMap;
        a.prefs.basemap_fade = 0.45;
        a.prefs.map_layers.push(MapLayer {
            name: "Parcels".into(),
            group: "Cadastre".into(),
            url: "https://x/MapServer".into(),
            opacity: 0.6,
            ..Default::default()
        });
        a.view.place([12.5, 40.0], 3.25);
        let bytes = write(&a).unwrap();
        let o = read(&bytes).unwrap();
        assert!(o.inp.contains(";CRS: EPSG:2100"));
        assert_eq!(o.project.map_layers, a.prefs.map_layers);

        // another machine, with other settings
        let mut b = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx));
        b.prefs.basemap = BasemapKind::None;
        open(&mut b, "Town.wepanet", &bytes, None);
        assert_eq!(b.net.nodes.len(), a.net.nodes.len());
        assert_eq!(b.file_name, "Town");
        assert!(b.project);
        assert_eq!(b.project_crs().as_deref(), Some("EPSG:2100"));
        assert_eq!(b.prefs.basemap, BasemapKind::OpenStreetMap);
        assert_eq!(b.prefs.basemap_fade, 0.45);
        assert_eq!(b.prefs.map_layers.len(), 1);
        assert_eq!(b.prefs.map_layers[0].opacity, 0.6);
        assert_eq!((b.view.center, b.view.scale), ([12.5, 40.0], 3.25));
        assert!(!b.dirty);
        // an INP opened after it is not a project
        b.load_document("Plain.inp", eui_core::examples::NET1, None);
        assert!(!b.project);
    }

    /// Save on a project writes the project file, not an INP file over it.
    #[test]
    fn save_writes_the_project_file() {
        let dir = std::env::temp_dir().join(format!("wepanet-project-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Town.wepanet");
        let ctx = egui::Context::default();
        let mut a = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx));
        a.load_example("Net1", eui_core::examples::NET1);
        a.prefs.map_layers.push(MapLayer {
            name: "Parcels".into(),
            ..Default::default()
        });
        a.project = true;
        a.path = Some(path.clone());
        a.save(false);
        let o = read(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(o.project.map_layers.len(), 1);
        assert!(!a.dirty);
        assert_eq!(a.file_name, "Town");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A new project starts without the last one's map layers.
    #[test]
    fn a_new_project_has_no_map_layers() {
        let ctx = egui::Context::default();
        let mut a = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx));
        a.load_example("Net1", eui_core::examples::NET1);
        a.prefs.map_layers.push(MapLayer::default());
        a.new_project();
        assert!(a.prefs.map_layers.is_empty());
    }

    /// Map layers added and not saved with the project are changes to lose.
    #[test]
    fn unsaved_map_layers_are_changes() {
        let ctx = egui::Context::default();
        let mut a = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        a.load_example("Net1", eui_core::examples::NET1);
        assert!(!a.unsaved());
        a.prefs.map_layers.push(MapLayer::default());
        assert!(a.unsaved());
        // saved with the project, and opened again: nothing to lose
        let bytes = write(&a).unwrap();
        let mut b = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx));
        open(&mut b, "Town.wepanet", &bytes, None);
        assert!(!b.unsaved());
        b.prefs.map_layers[0].opacity = 0.5;
        assert!(b.unsaved());
        // and the preferences do not keep them
        let json = serde_json::to_string(&b.prefs).unwrap();
        assert!(!json.contains("map_layers"));
    }

    #[test]
    fn not_a_project() {
        assert!(read(b"not a zip").is_err());
        assert!(is_project("A.WePaNeT") && !is_project("a.inp"));
    }
}
