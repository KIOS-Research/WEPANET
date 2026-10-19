//! EPANET-UI's Source Editor: a node's water quality source (strength, time
//! pattern and type), for EPANET's Source Quality and for the sources of
//! EPANET-MSX species, which have the same four types.

use egui::{RichText, Ui};
use eui_core::props::{Element, PropKey};

use crate::app::{Dialog, EpanetApp};

/// EPANET's source types, as written in the input files, and their names.
pub const TYPES: [(&str, &str); 4] = [
    ("CONCEN", "Concentration"),
    ("MASS", "Mass Booster"),
    ("SETPOINT", "Set Point Booster"),
    ("FLOWPACED", "Flow Paced Booster"),
];

/// Whose source is being edited.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// EPANET's Source Quality of a node.
    Quality(usize),
    /// The source of an EPANET-MSX species at a node.
    Species(usize, String),
}

/// The Source Editor's working copy.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceEdit {
    pub target: Target,
    pub strength: String,
    pub pattern: String,
    /// Index into [`TYPES`].
    pub kind: usize,
    pub error: Option<String>,
}

/// Splits `TYPE strength [pattern]` (the type may be left out).
fn parse(text: &str) -> (usize, String, String) {
    let tok: Vec<&str> = text.split_whitespace().collect();
    let (kind, rest) = match tok.first() {
        Some(t) if !eui_core::num::is_number(t) => (
            TYPES
                .iter()
                .position(|(k, _)| k.eq_ignore_ascii_case(t))
                .unwrap_or(0),
            &tok[1..],
        ),
        _ => (0, &tok[..]),
    };
    let get = |k: usize| rest.get(k).map_or_else(String::new, |s| s.to_string());
    (kind, get(0), get(1))
}

/// The current source of a target, as `TYPE strength [pattern]`.
fn current(app: &EpanetApp, target: &Target) -> String {
    match target {
        Target::Quality(i) => app
            .net
            .element_props(Element::Node(*i))
            .into_iter()
            .find(|p| p.key == PropKey::KeyedRest("SOURCES"))
            .map(|p| p.value)
            .unwrap_or_default(),
        Target::Species(i, sp) => app
            .net
            .msx
            .as_ref()
            .and_then(|m| m.source(&app.net.nodes[*i].id, sp))
            .unwrap_or_default(),
    }
}

/// Opens the Source Editor for a node's source.
pub fn open(app: &mut EpanetApp, target: Target) {
    let (kind, strength, pattern) = parse(&current(app, &target));
    app.source_edit = Some(SourceEdit {
        target,
        strength: crate::style::show_num(&strength),
        pattern,
        kind,
        error: None,
    });
    app.dialog = Dialog::SourceEditor;
}

/// The patterns a source can follow: the network's for EPANET, the MSX
/// file's own for a species.
fn patterns(app: &EpanetApp, target: &Target) -> Vec<String> {
    match target {
        Target::Quality(_) => app.net.patterns().into_iter().map(|p| p.0).collect(),
        Target::Species(..) => app
            .net
            .msx
            .as_ref()
            .map(|m| m.model().patterns)
            .unwrap_or_default(),
    }
}

/// Applies the editor's source (an empty strength removes it).
fn apply(app: &mut EpanetApp, e: &SourceEdit) -> Result<(), String> {
    let strength = eui_core::num::normalize(&e.strength);
    // EPANET-UI: a strength of 0 removes the source
    let text = if strength.is_empty() || strength.parse::<f64>() == Ok(0.0) {
        String::new()
    } else {
        strength
            .parse::<f64>()
            .map_err(|_| format!("\"{}\" is not a valid number", e.strength.trim()))?;
        let mut t = format!("{} {strength}", TYPES[e.kind].0);
        if !e.pattern.trim().is_empty() {
            t += &format!(" {}", e.pattern.trim());
        }
        t
    };
    match &e.target {
        Target::Quality(i) => {
            let mut trial = app.net.clone();
            trial.set_prop(Element::Node(*i), PropKey::KeyedRest("SOURCES"), &text)?;
            if trial != app.net {
                app.checkpoint();
                app.net = trial;
            }
            Ok(())
        }
        Target::Species(i, sp) => {
            let node = app.net.nodes[*i].id.clone();
            crate::msx::edit(app, |m| m.set_source(&node, sp, &text))
        }
    }
}

/// The Source Editor dialog. Returns true to close.
pub fn dialog(app: &mut EpanetApp, ui: &mut Ui) -> bool {
    let Some(mut e) = app.source_edit.clone() else {
        return true;
    };
    let Some(node) = app.net.nodes.get(match &e.target {
        Target::Quality(i) | Target::Species(i, _) => *i,
    }) else {
        app.source_edit = None;
        return true;
    };
    ui.set_width(250.0);
    if let Target::Species(_, sp) = &e.target {
        ui.label(RichText::new(format!("{sp} at Node {} (EPANET-MSX)", node.id)).strong());
    } else {
        ui.label(RichText::new(format!("Node {}", node.id)).strong());
    }
    ui.add_space(6.0);
    let msx_patterns = match &e.target {
        Target::Species(..) => Some(patterns(app, &e.target)),
        Target::Quality(_) => None,
    };
    let mut pick_pattern = false;
    egui::Grid::new("source_editor")
        .num_columns(2)
        .spacing([14.0, 10.0])
        .show(ui, |ui| {
            ui.label("Source Strength");
            ui.add(egui::TextEdit::singleline(&mut e.strength).desired_width(100.0))
                .on_hover_text("Enter 0 or leave empty to remove the source");
            ui.end_row();
            ui.label("Source Pattern");
            ui.horizontal(|ui| {
                match &msx_patterns {
                    // the MSX file's own patterns
                    Some(pats) => {
                        let shown = if e.pattern.is_empty() {
                            "(none)".to_string()
                        } else {
                            e.pattern.clone()
                        };
                        egui::ComboBox::from_id_salt("source_pattern")
                            .selected_text(shown)
                            .width(100.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut e.pattern, String::new(), "(none)");
                                for p in pats {
                                    ui.selectable_value(&mut e.pattern, p.clone(), p);
                                }
                            });
                    }
                    None => {
                        ui.add(egui::TextEdit::singleline(&mut e.pattern).desired_width(100.0));
                        pick_pattern = ui
                            .small_button("…")
                            .on_hover_text("Select a time pattern")
                            .clicked();
                    }
                }
            });
            ui.end_row();
            ui.label("Source Type");
            egui::ComboBox::from_id_salt("source_type")
                .selected_text(TYPES[e.kind].1)
                .width(130.0)
                .show_ui(ui, |ui| {
                    for (k, (_, name)) in TYPES.iter().enumerate() {
                        ui.selectable_value(&mut e.kind, k, *name);
                    }
                });
            ui.end_row();
        });
    if let Some(err) = &e.error {
        ui.add_space(6.0);
        ui.label(RichText::new(err).color(egui::Color32::from_rgb(0xc0, 0x39, 0x2b)));
    }
    ui.add_space(12.0);
    let mut close = false;
    ui.horizontal(|ui| {
        if ui
            .add_sized([70.0, 26.0], egui::Button::new("Help"))
            .clicked()
        {
            crate::help::open(ui.ctx(), "Source Quality");
        }
        ui.add_space(10.0);
        if ui
            .add_sized([70.0, 26.0], egui::Button::new("OK"))
            .clicked()
        {
            match apply(app, &e) {
                Ok(()) => close = true,
                Err(err) => e.error = Some(err),
            }
        }
        if ui
            .add_sized([70.0, 26.0], egui::Button::new("Cancel"))
            .clicked()
        {
            close = true;
        }
    });
    if close {
        app.source_edit = None;
    } else {
        app.source_edit = Some(e);
        if pick_pattern {
            crate::forms::open_patterns(app, crate::forms::Pick::SourcePattern);
        }
    }
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sources() {
        assert_eq!(parse("SETPOINT 1.2 P1"), (2, "1.2".into(), "P1".into()));
        assert_eq!(parse("2.5"), (0, "2.5".into(), String::new()));
        assert_eq!(parse(""), (0, String::new(), String::new()));
    }

    #[test]
    fn edits_epanet_and_msx_sources() {
        let ctx = egui::Context::default();
        let mut app = EpanetApp::new(&eframe::CreationContext::_new_kittest(ctx));
        app.load_example("Net1", eui_core::examples::NET1);
        let i = app.net.node_index("9").unwrap();
        open(&mut app, Target::Quality(i));
        let mut e = app.source_edit.clone().unwrap();
        e.strength = "1.5".into();
        e.kind = 2;
        e.pattern = "1".into();
        apply(&mut app, &e).unwrap();
        assert!(
            app.net.to_inp().contains("SETPOINT \t1.5 \t1") || {
                let inp = app.net.to_inp();
                inp.lines().any(|l| {
                    let t: Vec<&str> = l.split_whitespace().collect();
                    t == ["9", "SETPOINT", "1.5", "1"]
                })
            }
        );
        open(&mut app, Target::Quality(i));
        assert_eq!(app.source_edit.as_ref().unwrap().kind, 2);
        e.strength = "abc".into();
        assert!(apply(&mut app, &e).is_err());
        // an MSX species
        app.net.msx = Some(eui_core::MsxFile::new("a.msx", "[SPECIES]\n BULK CL2 MG\n"));
        let mut e = SourceEdit {
            target: Target::Species(i, "CL2".into()),
            strength: "0.8".into(),
            pattern: String::new(),
            kind: 1,
            error: None,
        };
        apply(&mut app, &e).unwrap();
        let m = app.net.msx.as_ref().unwrap();
        assert_eq!(m.source("9", "CL2").as_deref(), Some("MASS 0.8"));
        e.strength.clear();
        apply(&mut app, &e).unwrap();
        assert_eq!(app.net.msx.as_ref().unwrap().source("9", "CL2"), None);
    }
}
