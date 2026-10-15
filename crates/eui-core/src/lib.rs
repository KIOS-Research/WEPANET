//! EPANET-UI core library.
//!
//! Pure Rust, no I/O and no platform dependencies, so that the exact same
//! code runs in the desktop app, the CLI and the WebAssembly web app.
//!
//! * [`network`] – the editable network model and its INP reader/writer.
//! * [`results`] – reader for EPANET's binary output (`.out`) file.
//! * [`msx`]     – EPANET-MSX species and its binary output file.
//! * [`analysis`] – quantities derived from results, for the reports.
//! * [`units`]   – unit labels.

pub mod analysis;
pub mod data;
pub mod msx;
pub mod network;
pub mod props;
pub mod results;
pub mod units;

pub use data::{CURVE_TYPES, DataCurve, TimePattern};
pub use msx::{MsxCoefficient, MsxFile, MsxModel, MsxResults, MsxSpecies};
pub use network::{DataTable, Link, LinkKind, Network, Node, NodeKind};
pub use results::{LinkExtra, LinkVar, NodeExtra, NodeVar, SimResults};

/// The classic EPANET example networks, embedded so the app works with no files at hand.
pub mod examples {
    pub const NET1: &str = include_str!("../../../vendor/epanet/example-networks/Net1.inp");
    pub const NET2: &str = include_str!("../../../vendor/epanet/example-networks/Net2.inp");
    pub const NET3: &str = include_str!("../../../vendor/epanet/example-networks/Net3.inp");

    pub const ALL: &[(&str, &str)] = &[("Net1", NET1), ("Net2", NET2), ("Net3", NET3)];
}
