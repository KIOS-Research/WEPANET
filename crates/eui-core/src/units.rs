//! Unit labels, following EPANET's conventions.

/// EPANET flow-unit codes as stored in the binary output file.
pub const FLOW_UNITS: [&str; 11] = [
    "CFS", "GPM", "MGD", "IMGD", "AFD", "LPS", "LPM", "MLD", "CMH", "CMD", "CMS",
];

/// EPANET pressure-unit codes as stored in the binary output file.
pub const PRESSURE_UNITS: [&str; 5] = ["psi", "kPa", "m", "bar", "ft"];

pub fn is_si_flow(flow_units: &str) -> bool {
    matches!(
        flow_units.to_ascii_uppercase().as_str(),
        "LPS" | "LPM" | "MLD" | "CMH" | "CMD" | "CMS"
    )
}

pub fn length(si: bool) -> &'static str {
    if si { "m" } else { "ft" }
}

pub fn diameter(si: bool) -> &'static str {
    if si { "mm" } else { "in" }
}

pub fn velocity(si: bool) -> &'static str {
    if si { "m/s" } else { "ft/s" }
}

/// Head loss is reported per 1000 length units.
pub fn headloss(si: bool) -> &'static str {
    if si { "m/km" } else { "ft/kft" }
}
