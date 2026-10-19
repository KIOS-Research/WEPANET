//! Application state and actions. Drawing lives in the other modules:
//! `menu` (menu bar + toolbars + File menu), `viewer` (Map Viewer panel),
//! `explorer` (Project Explorer + Property Editor), `map`, `reports`,
//! `dialogs` and `status`.

use std::collections::HashMap;
use std::path::PathBuf;

use eui_core::props::{Element, OptionCategory};
use eui_core::{LinkKind, Network, NodeKind, SimResults};
use eui_engine::RunHandle;

use crate::basemap::BasemapKind;
use crate::io::{self, Inbox};
use crate::map::MapView;
use crate::style::{Legend, LinkTheme, MapBackground, NodeTheme};

pub type Selection = Element;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    AddNode(NodeKind),
    AddLink(LinkKind),
    /// Drag a selected link's vertices; click on the link to add one,
    /// right-click a vertex to remove it.
    Reshape,
    /// Click on the map to place a map label.
    AddLabel,
    /// Click on the map to see the features of the map layers there.
    Identify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MenuTab {
    Edit,
    Map,
    Project,
    Plugins,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewMode {
    Map,
    Report,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReportKind {
    Status,
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
}

impl ReportKind {
    /// The Report menu, in groups separated by a line (as in EPANET-UI).
    pub const MENU: [&'static [ReportKind]; 5] = [
        &[ReportKind::Status],
        &[
            ReportKind::Energy,
            ReportKind::Pumping,
            ReportKind::SystemFlow,
        ],
        &[
            ReportKind::TimeSeries,
            ReportKind::Percentiles,
            ReportKind::Profile,
        ],
        &[ReportKind::NodeTable, ReportKind::LinkTable],
        &[ReportKind::Calibration, ReportKind::FireFlow],
    ];
    pub fn label(self) -> &'static str {
        match self {
            ReportKind::Status => "Status Report",
            ReportKind::Energy => "Energy Report",
            ReportKind::Pumping => "Pumping Report",
            ReportKind::SystemFlow => "System Flow Report",
            ReportKind::TimeSeries => "Time Series Report",
            ReportKind::Percentiles => "Percentiles Plot",
            ReportKind::Profile => "Profile Plot",
            ReportKind::NodeTable => "Node Results Table",
            ReportKind::LinkTable => "Link Results Table",
            ReportKind::Calibration => "Calibration Report",
            ReportKind::FireFlow => "Fire Flow Report",
        }
    }
    pub fn icon(self) -> crate::icons::Icon {
        use crate::icons::Icon;
        match self {
            ReportKind::Status => Icon::StatusReport,
            ReportKind::Energy => Icon::Energy,
            ReportKind::Pumping => Icon::Pumping,
            ReportKind::SystemFlow => Icon::SystemFlow,
            ReportKind::TimeSeries => Icon::TimeSeries,
            ReportKind::Percentiles => Icon::Percentiles,
            ReportKind::Profile => Icon::Profile,
            ReportKind::NodeTable => Icon::NodeTable,
            ReportKind::LinkTable => Icon::LinkTable,
            ReportKind::Calibration => Icon::Calibration,
            ReportKind::FireFlow => Icon::FireFlow,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplorerItem {
    Title,
    Options(OptionCategory),
    Nodes,
    Links,
    Patterns,
    Curves,
    SimpleControls,
    RuleControls,
    Labels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Dialog {
    #[default]
    None,
    Welcome,
    Summary,
    Locate,
    Query,
    MapSettings,
    NodeLegend,
    LinkLegend,
    Units,
    ErrorCodes,
    About,
    /// "Run was successful." and the like, after an analysis.
    RunStatus,
    Preferences,
    Basemap,
    AddLayer,
    ImportShapefile,
    Georeference,
    BasemapAlign,
    Setup,
    PluginMessage,
    PluginTable,
    Elevations,
    FireFlowCurve,
    Benchmarks,
    PluginManager,
    /// The project's EPANET-MSX file.
    Msx,
    /// EPANET-UI's Source Editor (a node's quality source).
    SourceEditor,
    PatternSelector,
    PatternEditor,
    CurveSelector,
    CurveEditor,
    /// The Single Species Water Quality Editor.
    QualityEditor,
    ConfirmDelete,
    ConfirmReplace,
    /// F5 / Ctrl+R in the browser with changes not saved.
    ConfirmReload,
}

/// User preferences, persisted between sessions.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub dark: bool,
    pub blue_theme: bool,
    pub show_welcome: bool,
    pub recent: Vec<String>,
    pub menu_tab: MenuTab,
    pub show_viewer: bool,
    pub show_explorer: bool,
    /// Where the side panels are (the user moves them).
    pub docks: crate::dock::Layout,
    /// Layers from web map services over the map (see [`crate::layers`]):
    /// the project's, saved with it (.wepanet), not with the preferences.
    #[serde(skip)]
    pub map_layers: Vec<crate::layers::MapLayer>,
    /// The Map Layers panel closed (Map > Layers).
    pub hide_layers: bool,
    /// Map service addresses used before, latest first.
    pub layer_urls: Vec<String>,
    // map display settings (Map > Settings)
    /// Version of the map display defaults the settings were saved with.
    #[serde(default)]
    pub map_style: u32,
    pub node_size: f32,
    pub link_width: f32,
    pub node_border: bool,
    /// Node size grows with the node theme's legend class.
    pub node_proportional: bool,
    /// Link width grows with the link theme's legend class.
    pub link_proportional: bool,
    pub link_border: bool,
    pub show_node_ids: bool,
    pub show_node_values: bool,
    pub show_link_ids: bool,
    pub show_link_values: bool,
    /// Draw ID and value labels on a background-colored box.
    pub opaque_labels: bool,
    pub label_font: f32,
    /// Zoom-in steps past the full extent before ID and value labels appear.
    pub label_zoom: u32,
    pub show_arrows: bool,
    pub arrow_size: f32,
    /// Zoom-in steps past the full extent before flow arrows appear.
    pub arrow_zoom: u32,
    pub map_background: MapBackground,
    /// The Overview Map at the bottom of the Map Viewer.
    pub show_overview: bool,
    // basemap
    pub basemap: BasemapKind,
    pub show_basemap: bool,
    /// How far the basemap is faded toward the map background (0 to 0.9).
    pub basemap_fade: f32,
    pub basemap_gray: bool,
    /// Coordinate system of each file's map ("EPSG:nnnn" or a proj string).
    pub basemap_crs: HashMap<String, String>,
    /// Recently used coordinate systems ("6312" or a proj string), newest first.
    pub crs_recent: Vec<String>,
    // layers
    pub show_junctions: bool,
    pub show_tanks: bool,
    pub show_links: bool,
    pub show_pumps: bool,
    pub show_valves: bool,
    pub show_map_labels: bool,
    // themes
    pub node_theme: NodeTheme,
    pub link_theme: LinkTheme,
    pub node_legend_visible: bool,
    pub link_legend_visible: bool,
    pub node_legends: HashMap<NodeTheme, Legend>,
    pub link_legends: HashMap<LinkTheme, Legend>,
    /// Legend colors (RGB), shared by all node themes and by all link themes.
    pub node_colors: [[u8; 3]; 5],
    pub link_colors: [[u8; 3]; 5],
    /// Legends drawn with a frame on the map.
    pub node_legend_framed: bool,
    pub link_legend_framed: bool,
    /// Project Setup defaults for new objects and projects.
    pub setup: crate::setup::SetupDefaults,
    /// Loaded user plugins: (file name, Rhai source).
    pub user_plugins: Vec<(String, String)>,
    /// Plugin Manager: the repositories it reads, and the version of each
    /// plugin installed from one (by plugin name).
    pub plugin_repos: Vec<crate::plugin_manager::Repository>,
    pub plugin_versions: std::collections::BTreeMap<String, String>,
    /// Installed plugins unticked in the Plugin Manager: kept, but off the toolbar.
    pub disabled_plugins: std::collections::BTreeSet<String>,
    /// Core plugins (see `plugin_manager::CORE`) installed on the toolbar.
    pub core_plugins: std::collections::BTreeSet<String>,
    /// Icons (PNG) of plugins installed from a repository, by plugin name.
    pub plugin_icons: std::collections::BTreeMap<String, Vec<u8>>,
    /// The other files of plugins installed from a ZIP (sub-folders too), by
    /// plugin name: (path in the plugin's folder, bytes).
    #[serde(with = "crate::scripting::files_b64")]
    pub plugin_files: std::collections::BTreeMap<String, Vec<(String, Vec<u8>)>>,
    /// The `metadata.txt` of each installed plugin that has one, by plugin name.
    pub plugin_meta: std::collections::BTreeMap<String, crate::plugin_meta::Metadata>,
    pub animate_fps: f32,
    pub auto_length: bool,
    // Program Preferences
    pub blink_marker: bool,
    pub map_hints: bool,
    pub confirm_delete: bool,
    pub open_last: bool,
    pub auto_backup: bool,
    /// Decimal places shown in reports and computed results.
    pub decimals: usize,
    /// Decimal point or comma in what is shown and typed.
    pub decimal_separator: crate::style::DecimalSeparator,
    /// Interface language code (see `i18n`).
    pub language: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            dark: false,
            blue_theme: false,
            show_welcome: true,
            recent: Vec::new(),
            menu_tab: MenuTab::Project,
            show_viewer: true,
            show_explorer: true,
            docks: Default::default(),
            map_layers: Vec::new(),
            hide_layers: false,
            layer_urls: Vec::new(),
            map_style: MAP_STYLE,
            node_size: 3.0,
            link_width: 1.0,
            node_border: true,
            node_proportional: false,
            link_proportional: false,
            link_border: false,
            show_node_ids: false,
            show_node_values: false,
            show_link_ids: false,
            show_link_values: false,
            opaque_labels: false,
            label_font: 11.0,
            label_zoom: 0,
            show_arrows: true,
            arrow_size: 4.0,
            arrow_zoom: 0,
            map_background: MapBackground::White,
            show_overview: false,
            basemap: BasemapKind::None,
            show_basemap: true,
            basemap_fade: 0.3,
            basemap_gray: false,
            basemap_crs: HashMap::new(),
            crs_recent: Vec::new(),
            show_junctions: true,
            show_tanks: true,
            show_links: true,
            show_pumps: true,
            show_valves: true,
            show_map_labels: true,
            node_theme: NodeTheme::Elevation,
            link_theme: LinkTheme::Diameter,
            node_legend_visible: false,
            link_legend_visible: false,
            node_legends: HashMap::new(),
            link_legends: HashMap::new(),
            node_colors: crate::style::RAMP.map(rgb),
            link_colors: crate::style::RAMP.map(rgb),
            node_legend_framed: true,
            link_legend_framed: true,
            setup: Default::default(),
            user_plugins: Vec::new(),
            plugin_repos: crate::plugin_manager::default_repositories(),
            plugin_versions: Default::default(),
            disabled_plugins: Default::default(),
            core_plugins: Default::default(),
            plugin_icons: Default::default(),
            plugin_files: Default::default(),
            plugin_meta: Default::default(),
            animate_fps: 4.0,
            auto_length: false,
            blink_marker: true,
            map_hints: true,
            confirm_delete: true,
            open_last: false,
            auto_backup: false,
            decimals: 2,
            decimal_separator: Default::default(),
            language: "en".into(),
        }
    }
}

const PREFS_KEY: &str = "epanet-ui-prefs";

/// A color as stored in the preferences.
pub(crate) fn rgb(c: egui::Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

pub(crate) fn color([r, g, b]: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}
/// Bumped when the map display defaults change, so saved settings pick them up.
const MAP_STYLE: u32 = 1;
const MAX_UNDO: usize = 100;
pub(crate) const MAX_RECENT: usize = 8;

/// What the Map Query found.
#[derive(Clone, Debug, Default)]
pub struct QueryState {
    pub nodes: bool,
    pub node_theme: Option<NodeTheme>,
    pub link_theme: Option<LinkTheme>,
    /// 0 below, 1 equal to, 2 above
    pub op: usize,
    pub value: String,
    /// Matching element indices, in ascending order.
    pub hits: Vec<usize>,
    pub active: bool,
}

pub struct EpanetApp {
    pub(crate) ctx: egui::Context,
    pub(crate) document_id: u64,
    pending_document: Option<(Network, String, Option<PathBuf>)>,
    saving: Option<(u64, u64)>,
    replace_after_save: bool,
    pub(crate) net: Network,
    pub(crate) file_name: String,
    pub(crate) path: Option<PathBuf>,
    pending_path: Option<PathBuf>,
    pub(crate) dirty: bool,
    undo: Vec<Network>,
    redo: Vec<Network>,
    /// Incremented on every edit; used to know when results went stale.
    pub(crate) generation: u64,

    pub(crate) results: Option<SimResults>,
    /// Energy balance sums of the last run, per hydraulic time step.
    pub(crate) energy_sums: Option<eui_core::analysis::EnergySums>,
    pub(crate) results_generation: u64,
    pub(crate) report: String,
    pub(crate) run: Option<RunHandle>,
    pub(crate) run_error: Option<String>,
    node_map: Vec<Option<usize>>,
    link_map: Vec<Option<usize>>,
    maps_key: (u64, u64),
    /// Cached extent, link ends and link boxes for drawing and hit-testing.
    pub(crate) geom: crate::geometry::Geometry,
    /// Incremented whenever new results arrive.
    pub(crate) results_id: u64,
    pub(crate) run_message: String,
    pub(crate) rpt: crate::reports::ReportState,
    pub(crate) catalog: crate::catalog::CatalogState,
    pub(crate) plugin_manager: crate::plugin_manager::ManagerState,
    pub(crate) data_view: crate::data_view::DataViewState,
    /// Preferences as they were when the Preferences dialog opened (for Cancel).
    pub(crate) prefs_backup: Option<Prefs>,

    pub(crate) period: usize,
    pub(crate) animating: bool,
    play_clock: f32,

    pub(crate) view: MapView,
    pub(crate) view_mode: ViewMode,
    pub(crate) report_kind: ReportKind,
    pub(crate) selection: Option<Selection>,
    pub(crate) tool: Tool,
    pub(crate) link_start: Option<usize>,
    /// A map label being typed: its position and text so far.
    pub(crate) label_edit: Option<([f64; 2], String)>,
    pub(crate) drag_node: Option<usize>,
    pub(crate) drag_vertex: Option<usize>,

    pub(crate) explorer: ExplorerItem,
    /// Property editor text buffers, keyed by property name, for `buffer_owner`.
    pub(crate) buffers: HashMap<String, String>,
    pub(crate) buffer_owner: Option<(Option<Selection>, ExplorerItem, u64)>,
    pub(crate) prop_hint: String,
    pub(crate) section_buffer: Option<(ExplorerItem, String)>,
    pub(crate) copied: Option<(Selection, Vec<String>)>,

    pub(crate) prefs: Prefs,
    pub(crate) status: String,
    pub(crate) dialog: Dialog,
    pub(crate) backstage: bool,
    pub(crate) locate_text: String,
    pub(crate) locate_nodes: bool,
    pub(crate) query: QueryState,
    pub(crate) legend_edit: crate::dialogs::LegendEdit,
    pub(crate) cursor: Option<[f64; 2]>,
    pub(crate) basemap: crate::basemap::BasemapState,
    pub(crate) layers: crate::layers::LayersState,
    pub(crate) plugins: crate::plugins::PluginsState,
    /// This project's ID prefixes and properties for new objects.
    pub(crate) setup: crate::setup::SetupDefaults,
    pub(crate) setup_edit: Option<crate::setup::SetupEdit>,
    /// Whether the document is a WEPANET project (a .wepanet file), which Save
    /// saves as one.
    pub(crate) project: bool,
    /// The map layers as last saved with the project (or opened), to tell
    /// whether they changed since.
    pub(crate) saved_layers: Vec<crate::layers::MapLayer>,
    /// What a project file being opened keeps besides its network, for when
    /// the network is in (and its image basemap's picture).
    pub(crate) pending_project: Option<(crate::project::Project, Option<Vec<u8>>)>,
    pub(crate) scripts: crate::scripting::ScriptState,
    pub(crate) python: crate::python::PythonState,
    pub(crate) export: crate::export::ExportState,
    pub(crate) shp_import: Option<crate::import_shp::ShpImport>,
    pub(crate) setup_job: Option<crate::setup::SetupJob>,
    /// The project's EPANET-MSX input file.
    pub(crate) msx: crate::msx::MsxState,
    /// EPANET-UI's data forms (patterns, curves, quality, controls).
    pub(crate) forms: crate::forms::FormsState,
    /// The Source Editor's working copy.
    pub(crate) source_edit: Option<crate::source::SourceEdit>,
    /// The MSX text of the run in progress (its results need it).
    run_msx: Option<String>,
    pub(crate) inbox: Inbox,
    title_sent: String,
}

impl EpanetApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, PREFS_KEY))
            .unwrap_or_default();
        if prefs.map_style < MAP_STYLE {
            // EPANET-UI's map look: thin links, outlined nodes.
            let d = Prefs::default();
            prefs.node_size = d.node_size;
            prefs.link_width = d.link_width;
            prefs.node_border = d.node_border;
            prefs.map_style = MAP_STYLE;
        }
        crate::plugin_manager::migrate_repositories(&mut prefs.plugin_repos);
        // The app always starts on the Project tab (not the last one used).
        prefs.menu_tab = MenuTab::Project;
        let dialog = if prefs.show_welcome {
            Dialog::Welcome
        } else {
            Dialog::None
        };
        let mut app = EpanetApp {
            ctx: cc.egui_ctx.clone(),
            document_id: 0,
            pending_document: None,
            saving: None,
            replace_after_save: false,
            net: Network::new(),
            file_name: "Untitled".into(),
            path: None,
            pending_path: None,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            generation: 0,
            results: None,
            energy_sums: None,
            results_generation: 0,
            report: String::new(),
            run: None,
            run_error: None,
            node_map: Vec::new(),
            link_map: Vec::new(),
            maps_key: (u64::MAX, 0),
            geom: Default::default(),
            results_id: 0,
            run_message: String::new(),
            rpt: Default::default(),
            catalog: Default::default(),
            plugin_manager: Default::default(),
            data_view: Default::default(),
            prefs_backup: None,
            period: 0,
            animating: false,
            play_clock: 0.0,
            view: MapView::default(),
            view_mode: ViewMode::Map,
            report_kind: ReportKind::Status,
            selection: None,
            tool: Tool::Select,
            link_start: None,
            label_edit: None,
            drag_node: None,
            drag_vertex: None,
            explorer: ExplorerItem::Title,
            buffers: HashMap::new(),
            buffer_owner: None,
            prop_hint: String::new(),
            section_buffer: None,
            copied: None,
            prefs,
            status: String::new(),
            dialog,
            backstage: false,
            locate_text: String::new(),
            locate_nodes: true,
            query: QueryState {
                nodes: true,
                ..Default::default()
            },
            legend_edit: Default::default(),
            cursor: None,
            basemap: Default::default(),
            layers: Default::default(),
            plugins: Default::default(),
            setup: Default::default(),
            scripts: Default::default(),
            python: Default::default(),
            export: Default::default(),
            shp_import: None,
            setup_edit: None,
            project: false,
            saved_layers: Vec::new(),
            pending_project: None,
            setup_job: None,
            msx: Default::default(),
            source_edit: None,
            forms: Default::default(),
            run_msx: None,
            inbox: Inbox::default(),
            title_sent: String::new(),
        };
        app.setup = app.prefs.setup.clone();
        crate::fonts::install(&cc.egui_ctx);
        app.apply_theme(&cc.egui_ctx);
        app
    }

    /// Opens the most recent project, if the user asked for that (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_last_file(&mut self) {
        if self.prefs.open_last
            && let Some(p) = self.prefs.recent.first().cloned()
            && std::path::Path::new(&p).exists()
        {
            self.open_path(PathBuf::from(p));
        }
    }

    /// Opens a file passed on the command line (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path(&mut self, path: PathBuf) {
        match io::read_path(&path) {
            Ok((name, bytes)) => {
                self.load_document(&name, &io::decode_text(&bytes), Some(path));
            }
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(if self.prefs.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
    }

    pub(crate) fn palette(&self) -> crate::style::Palette {
        crate::style::palette(self.prefs.dark, self.prefs.blue_theme)
    }

    /// The palette with the map background chosen in Map Display Settings.
    pub(crate) fn map_palette(&self) -> crate::style::Palette {
        crate::style::map_palette(
            self.prefs.dark,
            self.prefs.blue_theme,
            self.prefs.map_background,
        )
    }

    // ----- documents ------------------------------------------------------

    pub(crate) fn load_document(&mut self, name: &str, text: &str, path: Option<PathBuf>) {
        match Network::from_inp(text) {
            Ok(net) => {
                // Saving writes plain INP text, never back over a compressed file.
                let path = path.filter(|p| !p.to_string_lossy().ends_with(".gz"));
                self.request_document(net, name, path);
            }
            Err(e) => {
                self.status = format!("Could not load {name}: {e}");
            }
        }
    }

    fn set_network(&mut self, net: Network, name: &str) {
        io::forget_project_file();
        self.document_id += 1;
        // Dropping the handle detaches the old run; its completion cannot reach this document.
        self.run = None;
        self.saving = None;
        self.replace_after_save = false;
        self.net = net;
        self.file_name = name
            .trim_end_matches(".gz")
            .trim_end_matches(".inp")
            .trim_end_matches(".INP")
            .trim_end_matches(".wepanet")
            .to_string();
        self.path = None;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.generation += 1;
        self.results = None;
        self.report.clear();
        self.run_error = None;
        self.selection = None;
        self.link_start = None;
        self.drag_node = None;
        self.drag_vertex = None;
        self.label_edit = None;
        self.copied = None;
        self.buffers.clear();
        self.buffer_owner = None;
        self.period = 0;
        self.animating = false;
        self.view.fit_pending = true;
        self.view_mode = ViewMode::Map;
        self.explorer = ExplorerItem::Title;
        self.section_buffer = None;
        self.query = QueryState {
            nodes: true,
            ..Default::default()
        };
        self.backstage = false;
        self.dialog = Dialog::None;
        self.rpt = Default::default();
        // The MSX file came with the network (see accept_document).
        self.msx.enabled = self.net.msx.is_some();
        self.msx.edit = None;
        crate::msx::forget_legends(self);
        if self.net.msx.is_none() {
            use crate::style::{LinkTheme, NodeTheme};
            if matches!(self.prefs.node_theme, NodeTheme::Msx(_)) {
                self.prefs.node_theme = NodeTheme::Elevation;
            }
            if matches!(self.prefs.link_theme, LinkTheme::Msx(_)) {
                self.prefs.link_theme = LinkTheme::Diameter;
            }
        }
    }

    fn set_path(&mut self, path: Option<PathBuf>) {
        if let Some(p) = &path {
            let s = p.to_string_lossy().into_owned();
            self.prefs.recent.retain(|r| r != &s);
            self.prefs.recent.insert(0, s);
            self.prefs.recent.truncate(MAX_RECENT);
        }
        self.path = path;
    }

    /// The project's coordinate reference system ("EPSG:nnnn" or a proj
    /// string): the one its file names, else the one chosen before for a file
    /// of its name.
    pub(crate) fn project_crs(&self) -> Option<String> {
        self.net
            .crs
            .clone()
            .or_else(|| self.prefs.basemap_crs.get(&self.file_name).cloned())
    }

    /// Sets the project's coordinate reference system, which is saved with it.
    pub(crate) fn set_project_crs(&mut self, def: String) {
        if self.net.crs.as_deref() != Some(def.as_str()) {
            self.net.crs = Some(def.clone());
            self.touch();
        }
        self.prefs.basemap_crs.insert(self.file_name.clone(), def);
    }

    pub(crate) fn load_example(&mut self, name: &str, text: &str) {
        self.load_document(&format!("{name}.inp"), text, None);
    }

    pub(crate) fn new_project(&mut self) {
        let mut net = Network::new();
        self.prefs.setup.apply_new_project_options(&mut net);
        self.setup = self.prefs.setup.clone();
        self.request_document(net, "Untitled", None);
    }

    fn request_document(&mut self, net: Network, name: &str, path: Option<PathBuf>) {
        if self.pending_document.is_some() {
            self.status = "Resolve the pending project change first".into();
            return;
        }
        self.pending_document = Some((net, name.to_string(), path));
        if self.dirty || self.saving.is_some() {
            self.dialog = Dialog::ConfirmReplace;
            self.backstage = false;
        } else {
            self.accept_document();
        }
    }

    fn accept_document(&mut self) {
        if let Some((mut net, name, path)) = self.pending_document.take() {
            // its EPANET-MSX file: opened with it, or beside it on disk
            net.msx = self.msx.pending.take();
            #[cfg(not(target_arch = "wasm32"))]
            if net.msx.is_none() {
                net.msx = path.as_deref().and_then(crate::msx::sibling_file);
            }
            self.set_network(net, &name);
            self.set_path(path);
            self.status = format!("Opened {name}");
            // a project: what goes with its network
            self.project = false;
            if let Some((project, image)) = self.pending_project.take() {
                crate::project::apply(self, project, image);
                self.project = true;
            }
            if let Some(m) = &self.net.msx {
                self.status = format!("Opened {name} with its EPANET-MSX file {}", m.name);
            }
            if name == "Untitled" {
                // a new project starts without the last one's map layers
                self.prefs.map_layers.clear();
                self.saved_layers.clear();
                self.layers = Default::default();
                self.view.center = [0.0, 0.0];
                self.view.scale = 1.0;
                self.view.fit_pending = false;
                self.status = "New project — use Project > Add to place nodes and links".into();
            }
        }
    }

    pub(crate) fn resolve_document_change(&mut self, choice: usize) {
        match choice {
            0 => {
                self.replace_after_save = true;
                if self.saving.is_none() {
                    self.save(false);
                }
            }
            1 => self.accept_document(),
            _ => {
                self.pending_document = None;
                self.pending_project = None;
                self.msx.pending = None;
                self.replace_after_save = false;
                self.dialog = Dialog::None;
            }
        }
    }

    pub(crate) fn open(&mut self, ctx: &egui::Context) {
        if let Some(p) = io::open_dialog(&self.inbox, ctx) {
            self.pending_path = Some(p);
        }
    }

    /// Opens a project of the Recent Projects list: a path on desktop, a
    /// file name in the browser (see [`crate::recent`]).
    pub(crate) fn open_recent(&mut self, path: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        self.open_path(PathBuf::from(path));
        #[cfg(target_arch = "wasm32")]
        crate::recent::open(self, path);
    }

    /// Whether something would be lost on leaving: changes to the network,
    /// or map layers added or changed since the project was saved.
    pub(crate) fn unsaved(&self) -> bool {
        self.dirty || self.prefs.map_layers != self.saved_layers
    }

    /// Saves the document as a WEPANET project (.wepanet): to its file if it
    /// is one already, else where the user says.
    pub(crate) fn save_project(&mut self) {
        let ask = !self.project;
        self.write_project(ask);
    }

    fn write_project(&mut self, ask: bool) {
        if self.saving.is_some() {
            return;
        }
        let bytes = match crate::project::write(self) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("Save failed: {e}");
                return;
            }
        };
        let suggested = format!("{}.{}", self.file_name, crate::project::EXT);
        let token = (self.document_id, self.generation);
        match io::save_binary(
            bytes,
            &suggested,
            "WEPANET project",
            crate::project::EXT,
            self.path.as_deref(),
            ask,
            token,
            &self.ctx,
        ) {
            Ok(Some(p)) => {
                self.file_name = p
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.status = format!("Saved the project {}", self.file_name);
                self.set_path(Some(p));
                self.dirty = false;
                self.project = true;
                self.saved_layers = self.prefs.map_layers.clone();
                if self.replace_after_save {
                    self.accept_document();
                }
            }
            Ok(None) => {
                if cfg!(target_arch = "wasm32") {
                    self.saving = Some(token);
                    self.status = format!("Saving {suggested}…");
                } else {
                    self.replace_after_save = false;
                }
            }
            Err(e) => {
                self.replace_after_save = false;
                self.status = format!("Save failed: {e}");
            }
        }
    }

    pub(crate) fn save(&mut self, save_as: bool) {
        if self.saving.is_some() {
            return;
        }
        // a project's Save saves the project (Save As, an INP file)
        if self.project && !save_as {
            self.write_project(false);
            return;
        }
        let text = self.net.to_inp();
        #[cfg(not(target_arch = "wasm32"))]
        if self.prefs.auto_backup
            && !save_as
            && let Some(p) = &self.path
            && p.exists()
        {
            let _ = std::fs::copy(p, p.with_extension("bak"));
        }
        let suggested = format!("{}.inp", self.file_name);
        let token = (self.document_id, self.generation);
        match io::save_text(
            &text,
            &suggested,
            self.path.as_deref(),
            save_as,
            token,
            &self.ctx,
        ) {
            Ok(Some(p)) => {
                self.file_name = p
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.status = format!("Saved {}", self.file_name);
                // the EPANET-MSX file goes beside the INP file
                #[cfg(not(target_arch = "wasm32"))]
                match crate::msx::save_beside(self, &p) {
                    Ok(Some(name)) => self.status += &format!(" and {name}"),
                    Ok(None) => {}
                    Err(e) => self.status = format!("Saved the network, but not its MSX file: {e}"),
                }
                self.set_path(Some(p));
                self.dirty = false;
                if self.replace_after_save {
                    self.accept_document();
                }
            }
            Ok(None) => {
                if cfg!(target_arch = "wasm32") {
                    self.saving = Some(token);
                    self.status = format!("Saving {suggested}…");
                    // a browser allows one save dialog per click: the MSX
                    // file has its own Save As in the EPANET-MSX dialog
                    if self.net.msx.is_some() {
                        self.status += " (save its MSX file from Project > EPANET-MSX)";
                    }
                } else {
                    self.replace_after_save = false;
                }
            }
            Err(e) => {
                self.replace_after_save = false;
                self.status = format!("Save failed: {e}");
            }
        }
    }

    fn finish_save(&mut self, event: io::SaveEvent) {
        if let Some(token) = event.token {
            if self.saving != Some(token) || token.0 != self.document_id {
                return;
            }
            self.saving = None;
            match event.result {
                Ok(Some(name)) => {
                    self.dirty = token.1 != self.generation;
                    self.project = crate::project::is_project(&name);
                    if self.project {
                        self.saved_layers = self.prefs.map_layers.clone();
                    }
                    self.file_name = name
                        .trim_end_matches(".inp")
                        .trim_end_matches(".INP")
                        .trim_end_matches(".wepanet")
                        .to_string();
                    self.status = format!("Saved {name}");
                    if self.replace_after_save && !self.dirty {
                        self.accept_document();
                    } else {
                        self.replace_after_save = false;
                    }
                }
                Ok(None) => {
                    self.replace_after_save = false;
                    self.status = "Save cancelled".into();
                }
                Err(e) => {
                    self.replace_after_save = false;
                    self.status = format!("Save failed: {e}");
                }
            }
        } else {
            match event.result {
                Ok(Some(name)) => self.status = format!("Saved {name}"),
                Ok(None) => {}
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    pub(crate) fn save_pending(&self) -> bool {
        self.saving.is_some()
    }

    fn drain_inbox(&mut self) {
        for item in self.inbox.drain() {
            match item {
                Ok((name, bytes)) => {
                    let path = self.pending_path.take();
                    if name.to_ascii_lowercase().ends_with(".msx") {
                        // an MSX file dropped on the window joins the project
                        crate::msx::take_file(self, &name, &bytes);
                        continue;
                    }
                    if crate::project::is_project(&name) {
                        crate::project::open(self, &name, &bytes, path);
                        continue;
                    }
                    self.load_document(&name, &io::decode_text(&bytes), path);
                }
                Err(e) => {
                    self.pending_path = None;
                    self.status = e;
                }
            }
        }
    }

    // ----- editing --------------------------------------------------------

    /// Records an undo point before a change.
    pub(crate) fn checkpoint(&mut self) {
        self.undo.push(self.net.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.touch();
    }

    /// Marks the network as changed (call after edits that already have a checkpoint).
    pub(crate) fn touch(&mut self) {
        self.dirty = true;
        self.generation += 1;
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.net, prev));
            self.after_structural_change();
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.net, next));
            self.after_structural_change();
        }
    }

    fn after_structural_change(&mut self) {
        self.dirty = true;
        self.generation += 1;
        self.selection = self.selection.filter(|s| match *s {
            Element::Node(i) => i < self.net.nodes.len(),
            Element::Link(i) => i < self.net.links.len(),
        });
        self.link_start = None;
        self.drag_node = None;
        self.buffer_owner = None;
        self.section_buffer = None;
    }

    pub(crate) fn select(&mut self, sel: Option<Selection>) {
        self.selection = sel;
        match sel {
            Some(Element::Node(_)) => self.explorer = ExplorerItem::Nodes,
            Some(Element::Link(_)) => self.explorer = ExplorerItem::Links,
            None => {}
        }
    }

    /// Deletes the selection, asking first if the user wants deletions confirmed.
    pub(crate) fn request_delete(&mut self) {
        if self.selection.is_none() {
            return;
        }
        if self.prefs.confirm_delete {
            self.dialog = Dialog::ConfirmDelete;
        } else {
            self.delete_selection();
        }
    }

    pub(crate) fn delete_selection(&mut self) {
        let Some(sel) = self.selection else { return };
        self.checkpoint();
        match sel {
            Element::Node(i) => {
                self.status = format!("Deleted node {}", self.net.nodes[i].id);
                self.net.delete_node(i);
            }
            Element::Link(i) => {
                self.status = format!("Deleted link {}", self.net.links[i].id);
                self.net.delete_link(i);
            }
        }
        self.selection = None;
        self.buffer_owner = None;
    }

    pub(crate) fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.link_start = None;
        self.label_edit = None;
        self.view_mode = ViewMode::Map;
        self.status = match tool {
            Tool::Select => String::new(),
            Tool::AddNode(k) => format!("Click on the map to add a {} node (Esc to stop)", k.label()),
            Tool::AddLink(k) => format!("Click the start node, then the end node of the new {} (Esc to stop)", k.label().to_lowercase()),
            Tool::Reshape => "Drag a vertex to move it, click the link to add one, right-click a vertex to delete it".into(),
            Tool::AddLabel => "Click on the map to place a label (Esc to stop)".into(),
            Tool::Identify => "Click on the map to see the features of the map layers there (Esc to stop)".into(),
        };
    }

    /// Recomputes the length of pipes attached to a node (Auto-Length).
    pub(crate) fn update_lengths(&mut self, node: usize) {
        if !self.prefs.auto_length {
            return;
        }
        let id = self.net.nodes[node].id.clone();
        for i in 0..self.net.links.len() {
            let l = &self.net.links[i];
            if l.kind == LinkKind::Pipe
                && (l.from == id || l.to == id)
                && let Some(len) = self.net.link_map_length(i)
            {
                self.net.links[i].fields[0] = format!("{len:.2}");
            }
        }
    }

    pub(crate) fn copy_selection(&mut self) {
        let Some(sel) = self.selection else { return };
        let fields = match sel {
            Element::Node(i) => self.net.nodes[i].fields.clone(),
            Element::Link(i) => self.net.links[i].fields.clone(),
        };
        self.copied = Some((sel, fields));
        self.status = "Properties copied".into();
    }

    pub(crate) fn can_paste(&self) -> bool {
        match (self.copied.as_ref(), self.selection) {
            (Some((Element::Node(a), _)), Some(Element::Node(b))) => {
                self.net.nodes.get(*a).map(|n| n.kind) == Some(self.net.nodes[b].kind)
            }
            (Some((Element::Link(a), _)), Some(Element::Link(b))) => {
                self.net.links.get(*a).map(|l| l.kind) == Some(self.net.links[b].kind)
            }
            _ => false,
        }
    }

    pub(crate) fn paste_selection(&mut self) {
        if !self.can_paste() {
            return;
        }
        let (Some((_, fields)), Some(sel)) = (self.copied.clone(), self.selection) else {
            return;
        };
        self.checkpoint();
        match sel {
            Element::Node(i) => self.net.nodes[i].fields = fields,
            Element::Link(i) => {
                // Keep the pipe's own length; paste the rest.
                let keep = (self.net.links[i].kind == LinkKind::Pipe)
                    .then(|| self.net.links[i].field(0).to_string());
                self.net.links[i].fields = fields;
                if let Some(len) = keep {
                    self.net.links[i].fields[0] = len;
                }
            }
        }
        self.buffer_owner = None;
        self.status = "Properties pasted".into();
    }

    pub(crate) fn selected_link(&self) -> Option<usize> {
        match self.selection {
            Some(Element::Link(i)) => Some(i),
            _ => None,
        }
    }

    // ----- simulation -----------------------------------------------------

    pub(crate) fn is_running(&self) -> bool {
        self.run.is_some()
    }

    pub(crate) fn run_simulation(&mut self, ctx: &egui::Context) {
        if self.is_running() {
            return;
        }
        if self.net.nodes.is_empty() {
            self.status = "Nothing to analyze: the project has no nodes".into();
            return;
        }
        let ctx = ctx.clone();
        self.run_msx = crate::msx::run_input(self);
        // With EPANET-MSX modeling water quality, EPANET's own is not run.
        let inp = if self.run_msx.is_some() {
            let mut net = self.net.clone();
            net.set_option("OPTIONS", "QUALITY", "None");
            net.to_inp()
        } else {
            self.net.to_inp()
        };
        self.run = Some(eui_engine::spawn_run(
            inp,
            self.run_msx.clone(),
            move || ctx.request_repaint(),
        ));
        self.results_generation = self.generation;
        self.status = "Running analysis…".into();
        // Run Status shows the progress, with Cancel.
        self.dialog = Dialog::RunStatus;
    }

    pub(crate) fn poll_simulation(&mut self) {
        let Some(handle) = self.run.as_mut() else {
            return;
        };
        let Some(result) = handle.try_take() else {
            return;
        };
        self.run = None;
        self.run_error = None;
        if self.pending_document.is_none() {
            self.dialog = Dialog::RunStatus;
        }
        match result {
            Err(eui_engine::EngineError::Cancelled) => {
                self.results = None;
                self.status = "Analysis cancelled".into();
                self.run_message = "Run was cancelled.".into();
            }
            Err(e) => {
                self.results = None;
                self.run_error = Some(e.to_string());
                self.status = format!("Analysis failed: {e}");
                self.report = self.status.clone();
                self.run_message = "Run was unsuccessful\ndue to system error.".into();
            }
            Ok(mut out) => {
                self.report = std::mem::take(&mut out.report);
                if out.is_error() {
                    self.results = None;
                    self.run_error = Some(format!("Error {}", out.code));
                    self.status = format!(
                        "Analysis failed with error {} — see the Status Report",
                        out.code
                    );
                    self.run_message =
                        "Run was unsuccessful.\nSee Status Report for reasons.".into();
                    self.view_mode = ViewMode::Report;
                    self.report_kind = ReportKind::Status;
                    return;
                }
                match SimResults::from_out(&out.out) {
                    Ok(mut res) => {
                        res.set_extra(&out.extra);
                        let msx = self.run_msx.take();
                        let mut msx_failed = out.msx_code > 0;
                        if let Some(text) = msx.filter(|_| !msx_failed) {
                            match eui_core::MsxResults::from_out(&out.msx, &text) {
                                Ok(m) => res.msx = Some(m),
                                Err(e) => {
                                    msx_failed = true;
                                    self.report
                                        .push_str(&format!("\n  EPANET-MSX: no results ({e})\n"));
                                }
                            }
                        }
                        if msx_failed {
                            self.status = "EPANET-MSX failed — see the Status Report".into();
                            self.run_message = "The hydraulic analysis was successful,\nbut EPANET-MSX failed.\nSee Status Report for reasons.".into();
                        } else if out.code > 0 {
                            self.status =
                                "Analysis completed with warnings — see the Status Report".into();
                            self.run_message =
                                "Warning messages were generated.\nSee Status Report for details."
                                    .into();
                        } else {
                            self.status = "Analysis completed successfully".into();
                            self.run_message = "Run was successful.".into();
                        }
                        self.period = self.period.min(res.n_periods - 1);
                        self.results = Some(res);
                        self.energy_sums = out.energy;
                        self.results_id += 1;
                    }
                    Err(e) => {
                        self.results = None;
                        self.run_error = Some(e.to_string());
                        self.status = format!("Could not read results: {e}");
                        self.run_message =
                            "Run was unsuccessful.\nSee Status Report for reasons.".into();
                    }
                }
            }
        }
    }

    pub(crate) fn results_stale(&self) -> bool {
        self.results.is_some() && self.results_generation != self.generation
    }

    /// Brings the cached map geometry up to date with the network.
    pub(crate) fn refresh_geometry(&mut self) {
        self.geom
            .refresh(&self.net, self.document_id, self.generation);
    }

    /// Extent of the network's coordinates (cached).
    pub(crate) fn bounds(&self) -> Option<[f64; 4]> {
        self.geom.bounds
    }

    /// Maps network element indices to result indices.
    pub(crate) fn refresh_maps(&mut self) {
        let key = (self.generation, self.results_id);
        if self.maps_key == key {
            return;
        }
        self.maps_key = key;
        let Some(res) = &self.results else {
            self.node_map.clear();
            self.link_map.clear();
            return;
        };
        let nodes: HashMap<&str, usize> = res
            .node_ids
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let links: HashMap<&str, usize> = res
            .link_ids
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        self.node_map = self
            .net
            .nodes
            .iter()
            .map(|n| nodes.get(n.id.as_str()).copied())
            .collect();
        self.link_map = self
            .net
            .links
            .iter()
            .map(|l| links.get(l.id.as_str()).copied())
            .collect();
    }

    pub(crate) fn node_result_index(&self, i: usize) -> Option<usize> {
        self.node_map.get(i).copied().flatten()
    }

    pub(crate) fn link_result_index(&self, i: usize) -> Option<usize> {
        self.link_map.get(i).copied().flatten()
    }

    pub(crate) fn node_result(&self, i: usize, var: eui_core::NodeVar) -> Option<f32> {
        let r = self.node_result_index(i)?;
        Some(self.results.as_ref()?.node_slice(self.period, var)[r])
    }

    pub(crate) fn link_result(&self, i: usize, var: eui_core::LinkVar) -> Option<f32> {
        let r = self.link_result_index(i)?;
        Some(self.results.as_ref()?.link_slice(self.period, var)[r])
    }

    /// A node result the output file lacks (demand deficit, emitter flow, leakage).
    pub(crate) fn node_extra(&self, i: usize, v: eui_core::NodeExtra) -> Option<f32> {
        let r = self.node_result_index(i)?;
        self.results.as_ref()?.node_extra(self.period, v, r)
    }

    pub(crate) fn link_extra(&self, i: usize, v: eui_core::LinkExtra) -> Option<f32> {
        let r = self.link_result_index(i)?;
        self.results.as_ref()?.link_extra(self.period, v, r)
    }

    // ----- themes ---------------------------------------------------------

    pub(crate) fn node_theme_value_of(&self, theme: NodeTheme, i: usize) -> Option<f32> {
        let n = &self.net.nodes[i];
        match theme {
            NodeTheme::None => None,
            NodeTheme::Elevation => n.num(0).map(|v| v as f32),
            NodeTheme::BaseDemand => {
                (n.kind == NodeKind::Junction).then(|| n.num(1).unwrap_or(0.0) as f32)
            }
            NodeTheme::Msx(s) => {
                let r = self.node_result_index(i)?;
                crate::msx::theme_value(self, true, s as usize, r)
            }
            t => match t.extra() {
                Some(x) => self.node_extra(i, x),
                None => self.node_result(i, t.result_var()?),
            },
        }
    }

    pub(crate) fn link_theme_value_of(&self, theme: LinkTheme, i: usize) -> Option<f32> {
        let l = &self.net.links[i];
        match theme {
            LinkTheme::None => None,
            LinkTheme::Diameter => match l.kind {
                LinkKind::Pipe => l.num(1),
                LinkKind::Valve => l.num(0),
                LinkKind::Pump => None,
            }
            .map(|v| v as f32),
            LinkTheme::Length => (l.kind == LinkKind::Pipe)
                .then(|| l.num(0))
                .flatten()
                .map(|v| v as f32),
            LinkTheme::Roughness => (l.kind == LinkKind::Pipe)
                .then(|| l.num(2))
                .flatten()
                .map(|v| v as f32),
            LinkTheme::Msx(s) => {
                let r = self.link_result_index(i)?;
                crate::msx::theme_value(self, false, s as usize, r)
            }
            t => match t.extra() {
                Some(x) => self.link_extra(i, x),
                None => self.link_result(i, t.result_var()?),
            },
        }
    }

    pub(crate) fn node_theme_value(&self, i: usize) -> Option<f32> {
        self.node_theme_value_of(self.prefs.node_theme, i)
    }

    pub(crate) fn link_theme_value(&self, i: usize) -> Option<f32> {
        self.link_theme_value_of(self.prefs.link_theme, i)
    }

    pub(crate) fn node_legend(&self) -> Legend {
        let t = self.prefs.node_theme;
        let mut l = self
            .prefs
            .node_legends
            .get(&t)
            .copied()
            .unwrap_or_else(|| Legend::new(self.node_default_breaks(t)));
        l.colors = self.prefs.node_colors.map(color);
        l
    }

    pub(crate) fn link_legend(&self) -> Legend {
        let t = self.prefs.link_theme;
        let mut l = self
            .prefs
            .link_legends
            .get(&t)
            .copied()
            .unwrap_or_else(|| Legend::new(self.link_default_breaks(t)));
        l.colors = self.prefs.link_colors.map(color);
        l
    }

    /// The interval breaks a theme starts with: EPANET-UI's, or for an MSX
    /// species steps up to its largest value.
    pub(crate) fn node_default_breaks(&self, t: NodeTheme) -> [f32; 4] {
        match t {
            NodeTheme::Msx(s) => {
                crate::msx::default_breaks(self.results.as_ref(), s as usize, true)
            }
            _ => None,
        }
        .unwrap_or(t.default_breaks())
    }

    pub(crate) fn link_default_breaks(&self, t: LinkTheme) -> [f32; 4] {
        match t {
            LinkTheme::Msx(s) => {
                crate::msx::default_breaks(self.results.as_ref(), s as usize, false)
            }
            _ => None,
        }
        .unwrap_or(t.default_breaks())
    }

    /// A theme's name: its label, or the species ID of an MSX species.
    pub(crate) fn node_theme_name(&self, t: NodeTheme) -> String {
        match t {
            NodeTheme::Msx(s) => {
                crate::msx::theme_name(self, s as usize).unwrap_or_else(|| t.label().into())
            }
            t => t.label().into(),
        }
    }

    pub(crate) fn link_theme_name(&self, t: LinkTheme) -> String {
        match t {
            LinkTheme::Msx(s) => {
                crate::msx::theme_name(self, s as usize).unwrap_or_else(|| t.label().into())
            }
            t => t.label().into(),
        }
    }

    /// The node themes on offer: EPANET-UI's, then the MSX bulk species.
    pub(crate) fn node_themes(&self) -> Vec<NodeTheme> {
        // EPANET's own quality is not computed when EPANET-MSX runs
        let msx = self.msx_active();
        let mut all: Vec<NodeTheme> = NodeTheme::ALL
            .into_iter()
            .filter(|t| !(msx && *t == NodeTheme::Quality))
            .collect();
        // the species only while EPANET-MSX runs
        all.extend(
            crate::msx::result_species(self.results.as_ref().filter(|_| msx), true)
                .into_iter()
                .filter_map(|s| u8::try_from(s).ok().map(NodeTheme::Msx)),
        );
        all
    }

    /// The link themes on offer: EPANET-UI's, then all MSX species.
    pub(crate) fn link_themes(&self) -> Vec<LinkTheme> {
        let msx = self.msx_active();
        let mut all: Vec<LinkTheme> = LinkTheme::ALL
            .into_iter()
            .filter(|t| !(msx && *t == LinkTheme::Quality))
            .collect();
        all.extend(
            crate::msx::result_species(self.results.as_ref().filter(|_| msx), false)
                .into_iter()
                .filter_map(|s| u8::try_from(s).ok().map(LinkTheme::Msx)),
        );
        all
    }

    /// EPANET-MSX models the water quality: the project has an MSX file
    /// and it runs with each analysis. EPANET's single species quality is
    /// then left out.
    pub(crate) fn msx_active(&self) -> bool {
        self.msx.enabled && self.net.msx.is_some()
    }

    /// Marks the results as out of date (the network did not change, but
    /// the analysis would now give others).
    pub(crate) fn mark_results_stale(&mut self) {
        if self.results.is_some() {
            self.results_generation = self.generation.wrapping_sub(1);
        }
    }

    pub(crate) fn node_theme_units(&self, theme: NodeTheme) -> String {
        let si = self.net.is_si();
        match theme {
            NodeTheme::None => String::new(),
            NodeTheme::Elevation => eui_core::units::length(si).into(),
            NodeTheme::BaseDemand | NodeTheme::EmitterFlow | NodeTheme::Leakage => {
                self.net.flow_units()
            }
            NodeTheme::DemandDeficit => "%".into(),
            NodeTheme::Msx(s) => crate::msx::species_label(self.results.as_ref(), s as usize)
                .map(|l| l.1)
                .unwrap_or_default(),
            t => match (&self.results, t.result_var()) {
                (Some(r), Some(v)) => r.node_units(v),
                _ => match t {
                    NodeTheme::Demand => self.net.flow_units(),
                    NodeTheme::Head => eui_core::units::length(si).into(),
                    NodeTheme::Pressure => self.pressure_units(),
                    _ => String::new(),
                },
            },
        }
    }

    pub(crate) fn link_theme_units(&self, theme: LinkTheme) -> String {
        let si = self.net.is_si();
        match theme {
            LinkTheme::None | LinkTheme::Roughness => String::new(),
            LinkTheme::Diameter => eui_core::units::diameter(si).into(),
            LinkTheme::Length => eui_core::units::length(si).into(),
            LinkTheme::Leakage => self.net.flow_units(),
            LinkTheme::Msx(s) => crate::msx::species_label(self.results.as_ref(), s as usize)
                .map(|l| l.1)
                .unwrap_or_default(),
            t => match (&self.results, t.result_var()) {
                (Some(r), Some(v)) => r.link_units(v),
                _ => match t {
                    LinkTheme::Flow => self.net.flow_units(),
                    LinkTheme::Velocity => eui_core::units::velocity(si).into(),
                    LinkTheme::Headloss => eui_core::units::headloss(si).into(),
                    _ => String::new(),
                },
            },
        }
    }

    pub(crate) fn pressure_units(&self) -> String {
        match self
            .net
            .option("OPTIONS", "PRESSURE")
            .map(|s| s.to_ascii_uppercase())
        {
            Some(p) if p == "KPA" => "kPa".into(),
            Some(p) if p == "METERS" => "m".into(),
            Some(p) if p == "BAR" => "bar".into(),
            Some(p) if p == "FEET" => "ft".into(),
            Some(p) if p == "PSI" => "psi".into(),
            _ => {
                if self.net.is_si() {
                    "m".into()
                } else {
                    "psi".into()
                }
            }
        }
    }

    // ----- frame ----------------------------------------------------------

    fn animate(&mut self, ctx: &egui::Context) {
        let Some(res) = &self.results else {
            self.animating = false;
            return;
        };
        if !self.animating || res.n_periods < 2 {
            return;
        }
        self.play_clock += ctx.input(|i| i.stable_dt).min(0.25);
        let step = 1.0 / self.prefs.animate_fps.max(0.5);
        while self.play_clock >= step {
            self.play_clock -= step;
            self.period = (self.period + 1) % res.n_periods;
        }
        ctx.request_repaint();
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending_document.is_some() {
            return;
        }
        use egui::{Key, KeyboardShortcut, Modifiers};
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let cmd_shift = |k| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        if ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::N))) {
            self.new_project();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::O))) {
            self.open(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&cmd_shift(Key::S))) {
            self.save(true);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::S))) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.run_simulation(ctx);
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&cmd_shift(Key::Z)) || i.consume_shortcut(&cmd(Key::Y))
        }) {
            self.redo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::Z))) {
            self.undo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::C))) {
            self.copy_selection();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::V))) {
            self.paste_selection();
        }
        // Backspace too: the "delete" key on Mac keyboards sends Backspace.
        if ctx.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
        }) {
            self.request_delete();
        }
        // Esc closes the innermost thing open: a dialog (which handles the key
        // itself), the drawing tool or File menu, a report selector, a report.
        if self.dialog == Dialog::None
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            if self.tool != Tool::Select || self.backstage {
                self.set_tool(Tool::Select);
                self.backstage = false;
            } else if self.rpt.selector != crate::reports::Selector::None {
                crate::reports::close_selector(self);
            } else if self.view_mode == ViewMode::Report {
                self.view_mode = ViewMode::Map;
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}: {}{}",
            crate::APP_NAME,
            self.file_name,
            if self.unsaved() { "*" } else { "" }
        );
        if title != self.title_sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_sent = title;
        }
    }
}

impl eframe::App for EpanetApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(p) = io::take_dropped(&self.inbox, ctx) {
            self.pending_path = Some(p);
        }
        self.drain_inbox();
        #[cfg(target_arch = "wasm32")]
        crate::recent::update(self);
        #[cfg(target_arch = "wasm32")]
        io::mark_unsaved(self.unsaved());
        #[cfg(target_arch = "wasm32")]
        if io::take_reload_request() && self.unsaved() {
            self.dialog = Dialog::ConfirmReload;
        }
        crate::style::set_decimals(self.prefs.decimals);
        crate::style::set_decimal_comma(self.prefs.decimal_separator.is_comma());
        crate::i18n::set(&self.prefs.language);
        for event in io::take_save_events() {
            self.finish_save(event);
        }
        if let Some((name, bytes)) = crate::catalog::poll(self) {
            self.load_document(&name, &io::decode_text(&bytes), None);
        }
        self.poll_simulation();
        crate::fireflow::poll(self, ctx);
        crate::plugins::poll(self, ctx);
        crate::setup::poll(self);
        crate::scripting::poll(self);
        crate::python::poll(self, ctx);
        crate::msx::poll(self);
        crate::export::poll(self, ctx);
        self.refresh_maps();
        self.refresh_geometry();
        self.animate(ctx);
        self.shortcuts(ctx);
        self.update_title(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::menu::show(self, ui);
        crate::status::show(self, ui);
        // Project ▸ Details takes the whole area below the toolbar.
        if self.data_view.open {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(self.palette().form))
                .show(ui, |ui| crate::data_view::show(self, ui));
            if self.backstage {
                crate::menu::file_menu(self, ui.ctx());
            }
            crate::dialogs::show(self, ui.ctx());
            return;
        }
        crate::dock::show(self, ui);
        // under the map, between the side panels
        crate::python::console::show(self, ui);
        // The map sits inset in the window, framed, as in EPANET-UI.
        let form = self.palette().form;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(form).inner_margin(6))
            .show(ui, |ui| match self.view_mode {
                ViewMode::Map => crate::map::show(self, ui),
                ViewMode::Report => crate::reports::show(self, ui),
            });
        if self.backstage {
            crate::menu::file_menu(self, ui.ctx());
        }
        crate::dialogs::show(self, ui.ctx());
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFS_KEY, &self.prefs);
    }

    /// Often: in the browser the preferences are only saved this way, not
    /// when the page closes, so a reload lost what changed in the last half
    /// minute.
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(3)
    }
}

#[cfg(test)]
mod document_tests {
    use super::*;

    fn app() -> EpanetApp {
        EpanetApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ))
    }

    /// The coordinate reference system is saved in the project's file and
    /// read back from it, wherever the file goes.
    #[test]
    fn the_coordinate_reference_system_goes_with_the_project() {
        let mut a = app();
        a.load_example("Net1", eui_core::examples::NET1);
        assert_eq!(a.project_crs(), None);
        a.set_project_crs("EPSG:2100".into());
        assert!(a.dirty);
        let text = a.net.to_inp();
        assert!(text.contains(";CRS: EPSG:2100"));
        // another machine: nothing remembered for this file name
        let mut b = app();
        b.load_document("Elsewhere.inp", &text, None);
        assert_eq!(b.project_crs().as_deref(), Some("EPSG:2100"));
        assert_eq!(crate::basemap::crs_def(&b).as_deref(), Some("EPSG:2100"));
        assert!(!b.dirty);
    }

    #[test]
    fn new_project_requires_discard_or_cancel_for_unsaved_changes() {
        let mut app = app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.checkpoint();
        let original = app.net.clone();
        app.new_project();
        assert_eq!(app.dialog, Dialog::ConfirmReplace);
        assert_eq!(app.net, original);
        app.resolve_document_change(2);
        assert_eq!(app.net, original);
        assert!(app.dirty);
        assert!(app.can_undo());
        app.new_project();
        app.resolve_document_change(1);
        assert!(app.net.nodes.is_empty());
        assert!(!app.dirty);
        assert!(!app.can_undo());
    }

    #[test]
    fn failed_or_cancelled_save_keeps_pending_document_and_unsaved_changes() {
        for result in [Ok(None), Err("permission denied".into())] {
            let mut app = app();
            app.load_example("Net1", eui_core::examples::NET1);
            app.checkpoint();
            app.new_project();
            let token = (app.document_id, app.generation);
            app.saving = Some(token);
            app.replace_after_save = true;
            app.finish_save(io::SaveEvent {
                token: Some(token),
                result,
            });
            assert!(app.dirty);
            assert!(app.pending_document.is_some());
            assert_eq!(app.dialog, Dialog::ConfirmReplace);
            assert!(!app.net.nodes.is_empty());
            assert!(app.saving.is_none());
        }
    }

    #[test]
    fn successful_save_continues_pending_document_change() {
        let mut app = app();
        app.load_example("Net1", eui_core::examples::NET1);
        app.checkpoint();
        app.new_project();
        let token = (app.document_id, app.generation);
        app.saving = Some(token);
        app.replace_after_save = true;
        app.finish_save(io::SaveEvent {
            token: Some(token),
            result: Ok(Some("Net1.inp".into())),
        });
        assert!(app.net.nodes.is_empty());
        assert!(!app.dirty);
        assert!(app.pending_document.is_none());
    }

    #[test]
    fn edits_during_save_are_not_marked_saved() {
        let mut app = app();
        app.checkpoint();
        let token = (app.document_id, app.generation);
        app.saving = Some(token);
        app.pending_document = Some((Network::new(), "Other".into(), None));
        app.replace_after_save = true;
        app.touch();
        app.finish_save(io::SaveEvent {
            token: Some(token),
            result: Ok(Some("network.inp".into())),
        });
        assert!(app.dirty);
        assert!(app.pending_document.is_some());
        assert_eq!(app.file_name, "network");
    }

    #[test]
    fn replacing_document_detaches_run_and_ignores_old_save_completion() {
        let mut app = app();
        app.load_example("Net1", eui_core::examples::NET1);
        let ctx = app.ctx.clone();
        app.run_simulation(&ctx);
        assert!(app.run.is_some());
        let token = (app.document_id, app.generation);
        app.saving = Some(token);
        app.set_network(Network::new(), "Other");
        app.finish_save(io::SaveEvent {
            token: Some(token),
            result: Ok(Some("old.inp".into())),
        });
        app.poll_simulation();
        assert!(app.run.is_none());
        assert!(app.results.is_none());
        assert_eq!(app.file_name, "Other");
    }
}
