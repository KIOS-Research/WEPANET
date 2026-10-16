// Count — the number of junctions or pipes.
// A port of the Count plugin of EPANET-Plugins by Elad Salomons
// (https://github.com/eladsal/EPANET-Plugins).

fn name() { "Count" }

fn description() { "Counts the junctions or the pipes of the network" }

fn menu() { ["Junctions", "Pipes"] }

fn run(item) {
    let kind = if item == "Junctions" { "Junction" } else { "Pipe" };
    let n = 0;
    if item == "Junctions" {
        for id in node_ids() {
            if node_type(id) == kind { n += 1; }
        }
    } else {
        for id in link_ids() {
            if link_type(id) == kind { n += 1; }
        }
    }
    message(`Number of ${item}: ${n}`);
}
