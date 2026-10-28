//! Many objects selected on the map at once: by the Select a Region tool
//! (a polygon clicked or a rectangle dragged, Enter for the whole network,
//! or by Tag) or by Ctrl/Shift-clicks with the Select tool. Delete deletes
//! them all; Escape clears them.

use std::collections::BTreeSet;

use egui::{Color32, Pos2, Rect};
use eui_core::props::{Element, PropKey};

use crate::app::{EpanetApp, Tool};
use crate::group::{Drawn, Region, inside};
use crate::i18n::{tr, trf};

/// The objects selected together, by ID (deletions renumber the rest), and
/// the region being drawn.
#[derive(Default)]
pub(crate) struct Multi {
    nodes: BTreeSet<String>,
    links: BTreeSet<String>,
    region: Region,
    /// The Tag typed to select by.
    tag: String,
}

impl Multi {
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.links.is_empty()
    }
    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
        self.links.clear();
    }
    /// Drops the region being drawn.
    pub(crate) fn reset_region(&mut self) {
        self.region = Region::default();
    }
    /// How many nodes and links are selected.
    pub(crate) fn counts(&self) -> (usize, usize) {
        (self.nodes.len(), self.links.len())
    }
}

/// The selection color for a node (or link) selected together with others.
pub(crate) fn shade(app: &EpanetApp, node: bool, i: usize) -> Option<Color32> {
    let m = &app.multi;
    if m.is_empty() {
        return None;
    }
    let picked = if node {
        m.nodes.contains(&app.net.nodes[i].id)
    } else {
        m.links.contains(&app.net.links[i].id)
    };
    picked.then_some(crate::style::SELECTED)
}

/// A Ctrl- or Shift-click with the Select tool: the object clicked joins the
/// objects selected (with the one selected before), or leaves them.
pub(crate) fn toggle(app: &mut EpanetApp, hit: Option<Element>) {
    if let Some(e) = app.selection.take() {
        add(app, e);
    }
    app.editor_shown = None;
    let Some(e) = hit else { return };
    let (set, id) = match e {
        Element::Node(i) => (&mut app.multi.nodes, app.net.nodes[i].id.clone()),
        Element::Link(i) => (&mut app.multi.links, app.net.links[i].id.clone()),
    };
    if !set.remove(&id) {
        set.insert(id);
    }
    report(app);
}

fn add(app: &mut EpanetApp, e: Element) {
    match e {
        Element::Node(i) => app.multi.nodes.insert(app.net.nodes[i].id.clone()),
        Element::Link(i) => app.multi.links.insert(app.net.links[i].id.clone()),
    };
}

fn report(app: &mut EpanetApp) {
    let (n, l) = app.multi.counts();
    app.status = trf("{} nodes and {} links selected", &[&n, &l]);
}

/// Map input with the Select a Region tool: the region drawn selects the
/// nodes in it and the links with both ends in it, in place of the objects
/// selected before, or besides them with Shift held. True when taken.
pub(crate) fn map_input(app: &mut EpanetApp, rect: Rect, response: &egui::Response) -> bool {
    if app.tool != Tool::Region {
        return false;
    }
    let poly = match app.multi.region.input(&app.view, rect, response) {
        Drawn::Ignored => return response.clicked(),
        Drawn::Taken => return true,
        Drawn::Closed(poly) => Some(poly),
        Drawn::Whole => None,
    };
    if !response.ctx.input(|i| i.modifiers.shift) {
        app.multi.clear();
    }
    app.selection = None;
    app.editor_shown = None;
    let net = &app.net;
    let within = |i: usize| match &poly {
        None => true,
        Some(poly) => net.nodes[i].pos.is_some_and(|p| inside(poly, p)),
    };
    let nodes: Vec<String> = (0..net.nodes.len())
        .filter(|&i| within(i))
        .map(|i| net.nodes[i].id.clone())
        .collect();
    let links: Vec<String> = net
        .links
        .iter()
        .filter(|l| {
            net.link_ends(l)
                .is_some_and(|(a, b)| within(a) && within(b))
        })
        .map(|l| l.id.clone())
        .collect();
    app.multi.nodes.extend(nodes);
    app.multi.links.extend(links);
    report(app);
    true
}

/// Select by Tag, under the Select a Region tool's hint: the nodes and
/// links with the Tag typed, in place of those selected or, with Shift
/// held, besides them.
pub(crate) fn tag_ui(app: &mut EpanetApp, ui: &mut egui::Ui) {
    let w = ui.available_width();
    let field = ui.add(
        egui::TextEdit::singleline(&mut app.multi.tag)
            .hint_text(tr("Tag"))
            .desired_width(w),
    );
    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let button = ui.add_sized([w, 26.0], egui::Button::new(tr("Select by Tag")));
    if enter || button.clicked() {
        let add = ui.input(|i| i.modifiers.shift);
        select_tag(app, add);
    }
}

/// Selects the nodes and links whose Tag is the one typed (in any case).
fn select_tag(app: &mut EpanetApp, add: bool) {
    let tag = app.multi.tag.trim().to_string();
    if tag.is_empty() {
        app.status = tr("Enter the Tag of the objects to select").into();
        return;
    }
    if !add {
        app.multi.clear();
    }
    app.selection = None;
    app.editor_shown = None;
    let net = &app.net;
    let tagged = |e: Element| {
        net.get_prop(e, PropKey::Tag)
            .trim()
            .eq_ignore_ascii_case(&tag)
    };
    let nodes: Vec<String> = (0..net.nodes.len())
        .filter(|&i| tagged(Element::Node(i)))
        .map(|i| net.nodes[i].id.clone())
        .collect();
    let links: Vec<String> = (0..net.links.len())
        .filter(|&i| tagged(Element::Link(i)))
        .map(|i| net.links[i].id.clone())
        .collect();
    app.multi.nodes.extend(nodes);
    app.multi.links.extend(links);
    report(app);
}

/// The region being drawn with the Select a Region tool.
pub(crate) fn draw(app: &EpanetApp, painter: &egui::Painter, rect: Rect, hover: Option<Pos2>) {
    if app.tool == Tool::Region {
        app.multi.region.draw(&app.view, painter, rect, hover);
    }
}

/// Deletes the objects selected together: the nodes with their links, and
/// the other links.
pub(crate) fn delete(app: &mut EpanetApp) {
    if app.multi.is_empty() {
        return;
    }
    let nodes = std::mem::take(&mut app.multi.nodes);
    let links = std::mem::take(&mut app.multi.links);
    app.checkpoint();
    for id in &links {
        if let Some(i) = app.net.link_index(id) {
            app.net.delete_link(i);
        }
    }
    for id in &nodes {
        if let Some(i) = app.net.node_index(id) {
            app.net.delete_node(i);
        }
    }
    app.selection = None;
    app.editor_shown = None;
    app.buffer_owner = None;
    app.section_buffer = None;
    app.status = trf(
        "Deleted {} nodes and {} links",
        &[&nodes.len(), &links.len()],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::tests::{at, setup};
    use egui::vec2;

    fn ids(set: &BTreeSet<String>) -> Vec<&str> {
        set.iter().map(String::as_str).collect()
    }

    /// The Select a Region tool: a rectangle selects the nodes in it and
    /// the links with both ends in it; with Shift another region adds to
    /// them; Enter alone takes the whole network, which Delete deletes (as
    /// one step to undo).
    #[test]
    fn regions_select_objects() {
        let (mut run, mut app) = setup();
        app.prefs.confirm_delete = false;
        app.set_tool(Tool::Region);
        let (n22, n23) = (at(&app, "22"), at(&app, "23"));
        run.drag(&mut app, n22 - vec2(15.0, 15.0), n23 + vec2(15.0, 15.0));
        assert_eq!(ids(&app.multi.nodes), ["22", "23"]);
        assert_eq!(ids(&app.multi.links), ["22"]);
        assert_eq!(
            shade(&app, true, app.net.node_index("22").unwrap()),
            Some(crate::style::SELECTED)
        );
        assert_eq!(shade(&app, true, app.net.node_index("21").unwrap()), None);

        run.modifiers = egui::Modifiers::SHIFT;
        let n21 = at(&app, "21");
        run.drag(&mut app, n21 - vec2(15.0, 15.0), n21 + vec2(15.0, 15.0));
        run.modifiers = Default::default();
        assert_eq!(ids(&app.multi.nodes), ["21", "22", "23"]);
        assert_eq!(ids(&app.multi.links), ["22"]);

        run.key(&mut app, egui::Key::Enter);
        assert_eq!(
            app.multi.counts(),
            (app.net.nodes.len(), app.net.links.len())
        );
        assert_eq!(app.tool, Tool::Region);
        run.key(&mut app, egui::Key::Delete);
        assert_eq!((app.net.nodes.len(), app.net.links.len()), (0, 0));
        assert!(app.multi.is_empty());
        app.undo();
        assert!(app.net.node_index("22").is_some());
    }

    /// Ctrl-clicks with the Select tool gather objects (the one selected
    /// before too); Delete asks, then deletes them; Escape clears them.
    #[test]
    fn ctrl_clicks_gather_objects() {
        let (mut run, mut app) = setup();
        app.prefs.confirm_delete = true;
        let (n22, n23) = (at(&app, "22"), at(&app, "23"));
        run.click(&mut app, n22);
        assert_eq!(
            app.selection,
            Some(Element::Node(app.net.node_index("22").unwrap()))
        );
        run.modifiers = egui::Modifiers::COMMAND;
        run.click(&mut app, n23);
        run.modifiers = Default::default();
        assert_eq!(ids(&app.multi.nodes), ["22", "23"]);
        assert!(app.selection.is_none());

        run.key(&mut app, egui::Key::Escape);
        assert!(app.multi.is_empty());

        run.click(&mut app, n22);
        run.modifiers = egui::Modifiers::COMMAND;
        run.click(&mut app, n23);
        run.modifiers = Default::default();
        run.key(&mut app, egui::Key::Delete);
        assert!(app.net.node_index("22").is_some(), "not before Yes");
        run.press(&mut app, "Yes");
        assert!(app.net.node_index("22").is_none() && app.net.node_index("23").is_none());
    }

    /// Select by Tag (in any case) selects the nodes and links with that
    /// Tag; with Shift held it adds to those selected; Delete deletes them.
    #[test]
    fn tags_select_objects() {
        let (mut run, mut app) = setup();
        app.prefs.confirm_delete = false;
        for e in [
            Element::Node(app.net.node_index("22").unwrap()),
            Element::Node(app.net.node_index("23").unwrap()),
            Element::Link(app.net.link_index("22").unwrap()),
        ] {
            app.net.set_prop(e, PropKey::Tag, "North").unwrap();
        }
        app.set_tool(Tool::Region);
        app.multi.tag = "north".into();
        run.press(&mut app, "Select by Tag");
        assert_eq!(ids(&app.multi.nodes), ["22", "23"]);
        assert_eq!(ids(&app.multi.links), ["22"]);

        let n10 = app.net.node_index("10").unwrap();
        app.net
            .set_prop(Element::Node(n10), PropKey::Tag, "Source")
            .unwrap();
        app.multi.tag = "Source".into();
        run.modifiers = egui::Modifiers::SHIFT;
        run.press(&mut app, "Select by Tag");
        run.modifiers = Default::default();
        assert_eq!(ids(&app.multi.nodes), ["10", "22", "23"]);

        run.key(&mut app, egui::Key::Delete);
        assert!(
            ["10", "22", "23"]
                .iter()
                .all(|id| app.net.node_index(id).is_none())
        );
    }
}
