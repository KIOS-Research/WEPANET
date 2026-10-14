// Pipe Summary — total pipe length for each diameter, as a table.

fn name() { "Pipe Summary" }

fn description() { "Total pipe length and count for each diameter" }

fn menu() { ["Length by Diameter"] }

fn run(item) {
    let totals = #{};
    for id in link_ids() {
        if link_type(id) != "Pipe" { continue; }
        let d = `${link_value(id, "Diameter")}`;
        let len = link_value(id, "Length");
        if d in totals {
            totals[d].length += len;
            totals[d].count += 1;
        } else {
            totals[d] = #{ length: len, count: 1 };
        }
    }
    let rows = [];
    for d in totals.keys() {
        let t = totals[d];
        rows.push([d, t.count, t.length.round()]);
    }
    let u = if is_si() { "mm" } else { "in" };
    let l = if is_si() { "m" } else { "ft" };
    table("Pipe length by diameter", [`Diameter (${u})`, "Pipes", `Length (${l})`], rows);
}
