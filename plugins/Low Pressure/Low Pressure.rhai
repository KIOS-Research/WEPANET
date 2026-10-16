// Low Pressure — junctions whose lowest pressure over the simulation falls
// below a limit, worst first; the worst one is selected on the map.

fn name() { "Low Pressure" }

fn description() { "Junctions whose pressure drops below a limit" }

fn menu() { ["Below 20", "Below 30", "Below 40"] }

fn run(item) {
    if !has_results() {
        message("Run an analysis first (Project > Analyze).");
        return;
    }
    let limit = parse_float(item.sub_string(6));
    let rows = [];
    for id in node_ids() {
        if node_type(id) != "Junction" { continue; }
        let lowest = 1e9;
        let when = 0.0;
        let series = node_series(id, "Pressure");
        for p in 0..series.len() {
            if series[p] < lowest {
                lowest = series[p];
                when = period_hours(p);
            }
        }
        if lowest < limit {
            rows.push([id, lowest.round() , when]);
        }
    }
    if rows.is_empty() {
        message(`No junction has a pressure below ${limit}.`);
        return;
    }
    rows.sort(|a, b| if a[1] < b[1] { -1 } else if a[1] > b[1] { 1 } else { 0 });
    select_node(rows[0][0]);
    table(`Junctions with pressure below ${limit}`, ["Junction", "Lowest pressure", "At hour"], rows);
}
