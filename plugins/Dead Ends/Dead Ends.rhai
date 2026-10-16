// Dead Ends — junctions connected to only one link (where water can stagnate).

fn name() { "Dead Ends" }

fn description() { "Junctions connected to a single link" }

fn version() { "1.0" }

fn run(item) {
    let links = #{};
    for id in link_ids() {
        for n in link_nodes(id) {
            if n in links { links[n] += 1; } else { links[n] = 1; }
        }
    }
    let rows = [];
    for id in node_ids() {
        if node_type(id) != "Junction" { continue; }
        let k = if id in links { links[id] } else { 0 };
        if k <= 1 {
            rows.push([id, k, node_value(id, "Base Demand")]);
        }
    }
    if rows.is_empty() {
        message("The network has no dead ends.");
        return;
    }
    select_node(rows[0][0]);
    table("Dead-end junctions", ["Junction", "Links", "Base demand"], rows);
}
