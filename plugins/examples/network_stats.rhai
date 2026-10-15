// Network Statistics — sizes, total pipe length and elevation range at a glance.

fn name() { "Network Statistics" }

fn description() { "Element counts, total pipe length and elevation range" }

fn version() { "1.0" }

fn run(item) {
    let counts = #{ Junction: 0, Reservoir: 0, Tank: 0, Pipe: 0, Pump: 0, Valve: 0 };
    let zmin = 1e30;
    let zmax = -1e30;
    for id in node_ids() {
        let t = node_type(id);
        counts[t] += 1;
        if t == "Junction" {
            let z = node_value(id, "Elevation");
            if z < zmin { zmin = z; }
            if z > zmax { zmax = z; }
        }
    }
    let length = 0.0;
    for id in link_ids() {
        let t = link_type(id);
        counts[t] += 1;
        if t == "Pipe" { length += link_value(id, "Length"); }
    }
    let l = if is_si() { "m" } else { "ft" };
    let rows = [];
    for k in ["Junction", "Reservoir", "Tank", "Pipe", "Pump", "Valve"] {
        rows.push([`${k}s`, counts[k]]);
    }
    rows.push([`Total pipe length (${l})`, length.round()]);
    if counts.Junction > 0 {
        rows.push([`Junction elevation (${l})`, `${zmin} to ${zmax}`]);
    }
    rows.push(["Flow units", flow_units()]);
    table(`Statistics of ${title()}`, ["Item", "Value"], rows);
}
