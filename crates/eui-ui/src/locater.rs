//! The Object Locater panel, as EPANET-UI's `maplocater` frame: finds a node
//! or a link by its ID (and lists what is joined to it), or lists all the
//! tanks, reservoirs, water quality sources, pumps or valves. A click on a
//! listed item selects it on the map.

use egui::{Color32, Ui, vec2};
use eui_core::props::{Element, PropKey};
use eui_core::{LinkKind, NodeKind};

use crate::app::EpanetApp;
use crate::i18n::tr;

/// What to find, and the heading of the list for each.
const FIND: [(&str, &str); 7] = [
    ("a Node", "Adjacent Links"),
    ("a Link", "Adjacent Nodes"),
    ("all Tanks", "Tank Nodes"),
    ("all Reservoirs", "Reservoir Nodes"),
    ("all WQ Sources", "WQ Source Nodes"),
    ("all Pumps", "Pump Links"),
    ("all Valves", "Valve Links"),
];

#[derive(Clone, Debug, Default)]
pub struct LocateState {
    /// Shown as a panel.
    pub open: bool,
    /// An index of `FIND`.
    pub find: usize,
    pub name: String,
    /// The listed items.
    pub items: Vec<Element>,
    /// Why nothing was found, shown below the list.
    pub message: Option<&'static str>,
}

/// Shows the panel, cleared.
pub fn open(app: &mut EpanetApp) {
    app.locate = LocateState {
        open: true,
        ..Default::default()
    };
    app.prefs.docks.activate(crate::dock::Pane::Locate);
}

pub fn close(app: &mut EpanetApp) {
    app.locate.open = false;
}

/// The panel's contents.
pub fn pane(app: &mut EpanetApp, ui: &mut Ui) {
    let pal = app.palette();
    // EPANET-UI's frame is 239 wide: labels at 16, fields from 72 to 216
    let k = ui.available_width() / 239.0;
    let (label_w, field_w) = (56.0 * k, 144.0 * k);
    let mut find = None;
    let mut search = false;
    let mut picked = None;
    egui::Frame::NONE
        .fill(pal.form)
        .inner_margin(egui::Margin {
            left: (16.0 * k) as i8,
            right: 0,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(0.0, 12.0);
            let label = |ui: &mut Ui, text: &str| {
                ui.allocate_ui_with_layout(
                    vec2(label_w, ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(label_w);
                        ui.label(tr(text))
                    },
                );
            };
            let st = &mut app.locate;
            ui.horizontal(|ui| {
                label(ui, "Find");
                egui::ComboBox::from_id_salt("locate_find")
                    .selected_text(tr(FIND[st.find].0))
                    .width(field_w)
                    .show_ui(ui, |ui| {
                        for (n, (text, _)) in FIND.iter().enumerate() {
                            if ui.selectable_label(st.find == n, tr(text)).clicked() {
                                find = Some(n);
                            }
                        }
                    });
            });
            // a name only for a node or a link
            let named = st.find < 2;
            ui.horizontal(|ui| {
                if named {
                    label(ui, "Named");
                } else {
                    ui.add_space(label_w);
                }
                let r = ui.add_enabled(
                    named,
                    egui::TextEdit::singleline(&mut st.name).desired_width(field_w),
                );
                if r.changed() {
                    st.items.clear();
                    st.message = None;
                }
                // Enter finds it
                search = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if search {
                    r.request_focus();
                }
            });
            ui.label(tr(FIND[st.find].1));
            // the list, on white, as wide as the fields reach
            let white = if ui.visuals().dark_mode {
                ui.visuals().extreme_bg_color
            } else {
                Color32::WHITE
            };
            egui::Frame::NONE
                .fill(white)
                .stroke(egui::Stroke::new(1.0, Color32::from_gray(160)))
                .inner_margin(2)
                .show(ui, |ui| {
                    ui.set_width(label_w + field_w - 4.0);
                    egui::ScrollArea::vertical()
                        .id_salt("locate_items")
                        .auto_shrink(false)
                        .max_height(118.0 * k)
                        .min_scrolled_height(118.0 * k)
                        .show(ui, |ui| {
                            // IDs on the left, each row the list's width
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::Min),
                                |ui| {
                                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                    for &e in &st.items {
                                        let id = match e {
                                            Element::Node(i) => &app.net.nodes[i].id,
                                            Element::Link(i) => &app.net.links[i].id,
                                        };
                                        let on = app.selection == Some(e);
                                        if ui.selectable_label(on, id.as_str()).clicked() {
                                            picked = Some(e);
                                        }
                                    }
                                },
                            );
                        });
                });
            if let Some(m) = st.message {
                ui.label(
                    egui::RichText::new(tr(m))
                        .font(crate::fonts::bold(13.0))
                        .color(Color32::from_rgb(0xe0, 0x10, 0x10)),
                );
            }
        });
    if let Some(n) = find {
        app.locate.find = n;
        app.locate.name.clear();
        app.locate.items.clear();
        app.locate.message = None;
        if n >= 2 {
            list_all(app);
        }
    }
    if search && app.locate.find < 2 {
        locate(app);
    }
    if let Some(e) = picked {
        app.select(Some(e));
    }
}

/// Finds the node or link named, selects it, brings it into view and lists
/// the links or nodes joined to it.
fn locate(app: &mut EpanetApp) {
    let id = app.locate.name.trim().to_string();
    let found = if app.locate.find == 0 {
        app.net.node_index(&id).map(Element::Node)
    } else {
        app.net.link_index(&id).map(Element::Link)
    };
    let Some(e) = found else {
        app.locate.items.clear();
        app.locate.message = Some("There is no such object in the project.");
        return;
    };
    app.locate.message = None;
    app.locate.items = match e {
        Element::Node(i) => {
            let id = &app.net.nodes[i].id;
            (0..app.net.links.len())
                .filter(|&l| app.net.links[l].from == *id || app.net.links[l].to == *id)
                .map(Element::Link)
                .collect()
        }
        Element::Link(i) => {
            let l = &app.net.links[i];
            [&l.from, &l.to]
                .into_iter()
                .filter_map(|n| app.net.node_index(n))
                .map(Element::Node)
                .collect()
        }
    };
    app.select(Some(e));
    let pos = match e {
        Element::Node(i) => app.net.nodes[i].pos,
        Element::Link(i) => app.net.link_ends(&app.net.links[i]).and_then(|(a, b)| {
            let (pa, pb) = (app.net.nodes[a].pos?, app.net.nodes[b].pos?);
            Some([(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0])
        }),
    };
    if let Some(p) = pos {
        app.view.center = p;
    }
    app.view_mode = crate::app::ViewMode::Map;
}

/// Lists all the items of the kind chosen.
fn list_all(app: &mut EpanetApp) {
    let net = &app.net;
    let nodes = |kind: Option<NodeKind>| -> Vec<Element> {
        (0..net.nodes.len())
            .filter(|&i| match kind {
                Some(k) => net.nodes[i].kind == k,
                // a source of some strength
                None => net
                    .get_prop(Element::Node(i), PropKey::KeyedRest("SOURCES"))
                    .split_whitespace()
                    .nth(1)
                    .and_then(eui_core::num::parse)
                    .is_some_and(|s| s > 0.0),
            })
            .map(Element::Node)
            .collect()
    };
    let links = |kind: LinkKind| -> Vec<Element> {
        (0..net.links.len())
            .filter(|&i| net.links[i].kind == kind)
            .map(Element::Link)
            .collect()
    };
    let items = match app.locate.find {
        2 => nodes(Some(NodeKind::Tank)),
        3 => nodes(Some(NodeKind::Reservoir)),
        4 => nodes(None),
        5 => links(LinkKind::Pump),
        _ => links(LinkKind::Valve),
    };
    if items.is_empty() {
        app.locate.message = Some("There are no objects of that type in the project.");
    }
    app.locate.items = items;
}
