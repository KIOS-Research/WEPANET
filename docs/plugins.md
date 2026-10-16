# Writing WEPANET plugins

A WEPANET plugin is a single text file written in [Rhai](https://rhai.rs), a
small scripting language with a syntax close to JavaScript and Rust, or in
[Python](#python-plugins). The same file works in the browser and in the
desktop application; nothing has to be compiled.

A plugin lives in a folder named like the plugin, holding its script, named
the same way, its `metadata.txt` and its icon, `icon.png` (a PNG of about
64x64; optional):

```text
Low Pressure/
    Low Pressure.rhai
    metadata.txt
    icon.png
```

`metadata.txt` tells the Plugin Manager the plugin's version, authors,
changelog and links (optional, but every plugin should have one):

```ini
[general]
name=Low Pressure
version=1.1
description=Junctions whose pressure drops below a limit during the simulation
author=Your name, Another author
email=you@example.org
about=A longer text, shown in the Plugin Manager.
    A line that starts with a space continues the one above it.
tracker=https://github.com/you/your-plugin/issues
repository=https://github.com/you/your-plugin
homepage=https://…
tags=results, pressure
icon=icon.png
wepanetMinimumVersion=0.1
experimental=False
deprecated=False

changelog=2026-11-01  Low Pressure 1.1:
        Faster.
    2026-10-14  Low Pressure 1.0:
        First version.
```

`name`, `version`, `description` and `author` are mandatory, and `name` must
be the name the script gives itself. `wepanetMinimumVersion` and
`wepanetMaximumVersion` (optional) keep the plugin from being installed in
WEPANET versions it does not work with; `icon` names its icon in the folder.
Lines starting with `#` or `;` are comments. A Python plugin finds the file in
`wepanet.plugin_dir`.

Install plugins with **Plugins > Manage Plugins**: from a repository, or with
**Install from ZIP**, which takes a ZIP of the plugin's folder (or the script
alone). The folder may hold more than the script and the icon (sub-folders,
translations, data, Python modules): the whole of it is installed, and a Python
plugin finds it in `wepanet.plugin_dir`, from where it can also `import` its
modules. A plugin then appears on the Plugins toolbar with a
drop-down of its commands, and WEPANET remembers it for the next session
(**Remove Plugin** in its drop-down forgets it).

Ready-made examples are in [`plugins`](../plugins), one folder each, and all
of them can be installed from the Plugin Manager:

| Plugin | What it does |
|---|---|
| `Count` | Counts junctions or pipes (the Count plugin of [EPANET-Plugins](https://github.com/eladsal/EPANET-Plugins) by Elad Salomons) |
| `Network Statistics` | Element counts, total pipe length and junction elevation range |
| `Pipe Summary` | A table of pipe length and count for each diameter |
| `Dead Ends` | Junctions connected to a single link |
| `Low Pressure` | Junctions whose pressure drops below a limit during the simulation |
| `High Velocity` | Pipes whose velocity rises above a limit during the simulation |
| `EPyT Pressure Check` | A Python plugin (`.py`): the lowest junction pressures, from its own [EPyT](https://github.com/OpenWaterAnalytics/EPyT) analysis |

## The Plugin Manager and plugin repositories

Plugin Manager offers the plugins of **repositories**: XML
files listing plugins, read from their URLs. WEPANET's own repository is
[`plugins/plugins.xml`](../plugins/plugins.xml); add others under
**Settings** in the Plugin Manager. Installed plugins are offered an
**Upgrade** when a repository lists a higher version.

The Plugin Manager also offers WEPANET's **core plugins**, **Elevations** and
**Fire Flow Curve** (after Elad Salomons' EPANET-Plugins). They are part of the
application, so installing one only puts it on the Plugins toolbar. Unticking a
plugin in the list hides it from the toolbar without uninstalling it.

To publish plugins, put them and a `plugins.xml` anywhere they can be
downloaded from (a GitHub repository works; browsers need the server to allow
cross-origin requests, which `raw.githubusercontent.com` does). Each plugin's
`<download>` is a ZIP of its folder or for a plugin
that is just a script and its icon, the folder itself:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plugins>
  <plugin name="Dead Ends" version="1.0">
    <description>Junctions connected to a single link</description>
    <author>Your name</author>
    <!-- a ZIP (e.g. Dead%20Ends.zip) or the folder: a URL, or relative to this file -->
    <download>Dead Ends</download>
    <tags>network, water quality</tags>
    <homepage>https://…</homepage>
  </plugin>
</plugins>
```

The icon shows in the Plugin Manager and on the plugin's toolbar button (it is
kept with the plugin when installed). `name` must be the name the script gives
in `fn name()`. Raise `version`
whenever the script changes, in `plugins.xml` and in the plugin's
`metadata.txt` alike. The details the Plugin Manager shows (about, changelog,
bug tracker, code repository, email) come from `metadata.txt`; a repository
may also give them in `<about>`, `<changelog>`, `<tracker>`, `<repository>`,
`<email>`, `<experimental>` and `<deprecated>`.

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

### EPANET-MSX results

When the project has an EPANET-MSX file (**Project > EPANET-MSX**), each
analysis also computes the concentration of every species it declares.

| Function | Returns |
|---|---|
| `msx_species()` | the species IDs, e.g. `["AS3", "AS5", "AStot", "AS5s", "NH2CL"]` (empty without MSX results) |
| `node_species(id, species, p)` | the concentration of a bulk species at a node at period `p` |
| `link_species(id, species, p)` | the concentration of a species in a link at period `p` (wall species too) |

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

## Python plugins

A plugin can also be a `.py` file, loaded and installed like a Rhai one. It
declares its name and commands as constants and defines `run(item)`; the
`wepanet` module gives it the project and shows what it found. Anything else
is ordinary Python, with any package: NumPy, pandas, SciPy, or
[EPyT](https://github.com/OpenWaterAnalytics/EPyT), the EPANET Python toolkit,
to run analyses of its own.

```python
NAME = "Pressure Check"                 # shown on the Plugins toolbar
DESCRIPTION = "Lowest junction pressures"   # optional: the tooltip
MENU = ["Lowest 10", "Lowest 20"]       # optional: the commands (default ["Run"])
REQUIREMENTS = ["epyt"]                 # optional: packages from PyPI

import numpy as np
import wepanet
from epyt import epanet

def run(item):
    d = epanet(wepanet.inp_file, display_msg=False)
    h = d.getComputedHydraulicTimeSeries()
    lowest = np.asarray(h.Pressure).min(axis=0)
    ids = d.getNodeNameID()
    n = int(item.split()[1])
    rows = sorted(zip(ids, lowest.round(2)), key=lambda r: r[1])[:n]
    wepanet.table("Lowest pressures", ["Node", "Pressure"], rows)
    d.unload()
```

`NAME`, `DESCRIPTION`, `MENU` and `REQUIREMENTS` are read without running the
file, so they must be plain strings and lists of strings.

**Where it runs.** In the browser WEPANET runs Python itself with
[Pyodide](https://pyodide.org), loaded the first time a Python plugin runs
(it takes a few seconds, then it is ready). The packages of the Pyodide
distribution that the plugin imports (NumPy, pandas, SciPy, Matplotlib, …)
are loaded by themselves, and those in `REQUIREMENTS` are installed from
PyPI. On the desktop WEPANET runs the Python installed on the computer: the
one the `WEPANET_PYTHON` environment variable names, else `python`,
`python3` or `py` on the PATH. The packages are then installed as usual, e.g.
`python -m pip install epyt`; a plugin missing one says which.

**How it runs.** The plugin runs in a folder of its own holding the project
as an EPANET input file, `wepanet.inp_file`, and the results of the last
analysis; it may write files there. Everything it prints is shown when it
ends, and an error is shown with the plugin's own lines. Unlike a Rhai
plugin it is not sandboxed: on the desktop it can do whatever Python can, so
install only plugins you trust. A plugin changes the project by handing back
an input file, e.g. one saved by EPyT, with `wepanet.set_network(path)`; it
replaces the project as one step that **Undo** takes back.

### The `wepanet` module

| Name | What it gives or does |
|---|---|
| `inp_file` | the path of the project's input file |
| `item` | the command chosen (also passed to `run`) |
| `plugin_dir` | the plugin's own folder, with the other files of its ZIP (on `sys.path`) |
| `in_browser` | `True` when running in Pyodide |
| `node_ids()`, `link_ids()` | the IDs of the nodes and links |
| `node_type(id)`, `link_type(id)` | `"Junction"`, `"Reservoir"`, `"Tank"`; `"Pipe"`, `"Pump"`, `"Valve"` |
| `flow_units()` | e.g. `"GPM"`, `"LPS"` |
| `has_results()` | `True` after an analysis of the project as it is |
| `periods()`, `current_period()`, `period_hours(p)` | as in Rhai |
| `node_result(id, var, p)`, `node_series(id, var)` | WEPANET's results: `"Demand"`, `"Head"`, `"Pressure"`, `"Quality"` |
| `link_result(id, var, p)`, `link_series(id, var)` | `"Flow"`, `"Velocity"`, `"Headloss"`, `"Quality"`, `"Status"`, `"Setting"`, `"Reaction"`, `"Friction"` |
| `message(...)` | a line of the message shown when the plugin ends (as `print`) |
| `table(title, header, rows)` | a table window |
| `select_node(id)`, `select_link(id)` | selects the object on the map |
| `status(text)` | text for the status bar |
| `progress(text)` | shown on the status bar while the plugin runs |
| `set_network(path_or_text)` | replaces the project with an input file |

## The Python Console

**Plugins > Python Console** opens a panel under the map, Python
Console: Python lines run one after the other in one session, which keeps its
variables, with `wepanet` already imported. Each line sees the project as it
is then (an edit or an analysis since the last line is passed on first), and
what it asks WEPANET to show (a table, a selection, a new project with
`set_network`) is shown as for a plugin; what it prints is shown in the
console. A block (`for`, `def`, …) goes on with the `...` prompt and ends
with an empty line; Up and Down go through the lines typed before.

```python
>>> from epyt import epanet
>>> d = epanet(wepanet.inp_file, display_msg=False)
>>> d.getNodeCount()
11
```

`pip install <package>` installs a package (in the browser from PyPI, with
Pyodide's micropip). The console uses the same Python as the plugins: Pyodide
in the browser, the local Python on the desktop, where closing the console
ends its session.
