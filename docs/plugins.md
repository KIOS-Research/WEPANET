# Writing WEPANET plugins

A WEPANET plugin is a single text file written in [Rhai](https://rhai.rs), a
small scripting language with a syntax close to JavaScript and Rust. The same
file works in the browser and in the desktop application; nothing has to be
installed or compiled.

Install plugins with **Plugins > Manage Plugins**, or load a file with
**Plugins > Load Plugin**. A plugin then appears on the Plugins toolbar with a
drop-down of its commands, and WEPANET remembers it for the next session
(**Remove Plugin** in its drop-down forgets it).

Ready-made examples are in [`plugins/examples`](../plugins/examples), and all
of them can be installed from the Plugin Manager:

| File | What it does |
|---|---|
| `count.rhai` | Counts junctions or pipes (the Count plugin of [EPANET-Plugins](https://github.com/eladsal/EPANET-Plugins) by Elad Salomons) |
| `network_stats.rhai` | Element counts, total pipe length and junction elevation range |
| `pipe_summary.rhai` | A table of pipe length and count for each diameter |
| `dead_ends.rhai` | Junctions connected to a single link |
| `low_pressure.rhai` | Junctions whose pressure drops below a limit during the simulation |
| `high_velocity.rhai` | Pipes whose velocity rises above a limit during the simulation |

## The Plugin Manager and plugin repositories

As in QGIS, the Plugin Manager offers the plugins of **repositories**: XML
files listing plugins, read from their URLs. WEPANET's own repository is
[`plugins/plugins.xml`](../plugins/plugins.xml); add others under
**Settings** in the Plugin Manager. Installed plugins are offered an
**Upgrade** when a repository lists a higher version.

To publish plugins, put their `.rhai` files and a `plugins.xml` anywhere they
can be downloaded from (a GitHub repository works; browsers need the server to
allow cross-origin requests, which `raw.githubusercontent.com` does):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plugins>
  <plugin name="Dead Ends" version="1.0">
    <description>Junctions connected to a single link</description>
    <author>Your name</author>
    <file>examples/dead_ends.rhai</file>  <!-- a URL, or a path relative to this file -->
    <icon>examples/icons/dead_ends.png</icon>  <!-- PNG, about 64x64; optional -->
    <tags>network, water quality</tags>
    <homepage>https://…</homepage>
  </plugin>
</plugins>
```

The icon shows in the Plugin Manager and on the plugin's toolbar button (it is
kept with the plugin when installed). `name` must be the name the script gives
in `fn name()`. Raise `version`
whenever the script changes.

## The shape of a plugin

```rhai
// The name shown on the Plugins toolbar.
fn name() { "Count" }

// Optional: shown as the button's tooltip.
fn description() { "Counts the junctions or the pipes of the network" }

// Optional: the commands of the plugin's drop-down (default: ["Run"]).
fn menu() { ["Junctions", "Pipes"] }

// Called with the command chosen from the drop-down.
fn run(item) {
    let n = 0;
    for id in node_ids() {
        if node_type(id) == "Junction" { n += 1; }
    }
    message(`Number of junctions: ${n}`);
}
```

## How plugins run

- A plugin works on a **copy** of the project. If it changes anything, the copy
  replaces the project when the plugin ends, as one step that **Undo** takes back.
  If the plugin stops with an error, nothing is changed.
- Plugins are **sandboxed**: they can read and change the project and read the
  simulation results, but they cannot open files or reach the internet.
- A plugin that runs too long (an endless loop, for example) is stopped.
- Errors are shown with the line and column of the script where they happened.
- `print(...)` adds a line to the message shown when the plugin ends.

## API

IDs are the IDs shown in the Property Editor (text). Property names are those of
the Property Editor, e.g. `"Elevation"`, `"Base Demand"`, `"Length"`,
`"Diameter"`, `"Roughness"`, `"Initial Status"`; upper or lower case does not
matter. Values that read as numbers are returned as numbers, others as text.

### Network

| Function | Returns |
|---|---|
| `node_count()`, `link_count()` | number of nodes / links |
| `node_ids()`, `link_ids()` | array of IDs |
| `node_type(id)` | `"Junction"`, `"Reservoir"` or `"Tank"` |
| `link_type(id)` | `"Pipe"`, `"Pump"` or `"Valve"` |
| `link_nodes(id)` | `[start node, end node]` |
| `node_xy(id)` | `[x, y]`, or `()` if the node has no coordinates |
| `node_value(id, property)`, `link_value(id, property)` | a property's value |
| `set_node_value(id, property, value)`, `set_link_value(id, property, value)` | changes a property (checked as in the Property Editor) |
| `option(section, key)` | an option, e.g. `option("OPTIONS", "UNITS")`, `option("TIMES", "DURATION")`, or `()` |
| `set_option(section, key, value)` | sets an option |
| `flow_units()` | e.g. `"GPM"`, `"LPS"` |
| `is_si()` | `true` for SI (metric) units |
| `title()` | the project title |

### Results

These need an analysis whose results are up to date (run **Analyze** first).

| Function | Returns |
|---|---|
| `has_results()` | `true` if there are current results |
| `periods()` | number of reporting periods |
| `current_period()` | the period shown on the map |
| `period_hours(p)` | time of period `p`, in hours |
| `node_result(id, variable, p)` | `"Demand"`, `"Head"`, `"Pressure"` or `"Quality"` at period `p` |
| `link_result(id, variable, p)` | `"Flow"`, `"Velocity"`, `"Headloss"`, `"Quality"`, `"Status"`, `"Setting"`, `"Reaction"` or `"Friction"` at period `p` |
| `node_series(id, variable)`, `link_series(id, variable)` | the values for every period, as an array |

### Showing results

| Function | Effect |
|---|---|
| `message(text)` | a message box when the plugin ends (several calls add lines) |
| `table(title, header, rows)` | a table window, e.g. `table("Pipes", ["ID", "Length"], [["P1", 100], ["P2", 250]])` |
| `select_node(id)`, `select_link(id)` | selects the object on the map and in the Property Editor |
| `status(text)` | text for the status bar |

## Rhai in five lines

```rhai
let x = 5;                      // variables
let a = [1, 2, 3];  a.push(4);  // arrays
let m = #{ name: "P1", d: 12 }; // object maps: m.name, m["d"]
for v in a { print(v); }        // loops; also while, loop, break, continue
`Pressure ${x} psi`             // strings with ${...} interpolation
```

The full language is described in the [Rhai Book](https://rhai.rs/book/).
