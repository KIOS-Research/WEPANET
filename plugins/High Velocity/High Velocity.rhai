// High Velocity — pipes whose highest velocity during the simulation exceeds a
// limit (m/s or ft/s, as the project's units), fastest first.

fn name() { "High Velocity" }

fn description() { "Pipes whose velocity rises above a limit" }

fn version() { "1.0" }

fn menu() { ["Above 1", "Above 1.5", "Above 2", "Above 3"] }

fn run(item) {
    if !has_results() {
        message("Run an analysis first (Project > Analyze).");
        return;
    }
    let limit = parse_float(item.sub_string(6));
    let rows = [];
    for id in link_ids() {
        if link_type(id) != "Pipe" { continue; }
        let top = 0.0;
        let when = 0.0;
        let series = link_series(id, "Velocity");
        for p in 0..series.len() {
            if series[p] > top {
                top = series[p];
                when = period_hours(p);
            }
        }
        if top > limit {
            rows.push([id, (top * 100.0).round() / 100.0, when]);
        }
    }
    if rows.is_empty() {
        message(`No pipe is faster than ${limit}.`);
        return;
    }
    rows.sort(|a, b| if a[1] > b[1] { -1 } else if a[1] < b[1] { 1 } else { 0 });
    select_link(rows[0][0]);
    table(`Pipes with velocity above ${limit}`, ["Pipe", "Highest velocity", "At hour"], rows);
}
