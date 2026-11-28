"""WEPANET's Python: the runner of Python plugins and the Python Notebook.

WEPANET gives it a folder holding the project (network.inp), the request
(request.json) and, after an analysis, its results (results.bin); for a
plugin also the plugin (plugin.py) and, in plugin/, the other files of the
plugin's folder. It runs with the local Python on the desktop, with Pyodide
in the browser. Python code imports `wepanet` to read the project and its
results and to show what it found.

    python python_runner.py <folder>             runs plugin.py's run(item);
                                                 answers in response.json (and
                                                 network_out.inp when the plugin
                                                 changed the project)
    python python_runner.py --notebook <folder>  the Python Notebook: reads
                                                 {"cell", "reload", "dpi"} JSON lines,
                                                 answers "@@cell {...}" lines

In the browser the module is loaded once and `run_plugin(folder)` and
`run_cell(folder, source, reload, fresh, dpi)` are called.
"""

import array
import ast
import importlib.util
import io
import json
import linecache
import os
import re
import shutil
import sys
import traceback
import types
import warnings

NODE_VARS = ["demand", "head", "pressure", "quality"]
LINK_VARS = [
    "flow",
    "velocity",
    "headloss",
    "quality",
    "status",
    "setting",
    "reaction",
    "friction",
]

work = None
request = {}
response = {}
_results = None
_values = None


def _new_response():
    global response
    response = {
        "messages": [],
        "status": None,
        "table": None,
        "select": None,
        "error": None,
    }


def _write_line(line):
    out = sys.__stdout__
    line = line + "\n"
    try:
        if hasattr(out, "buffer"):
            out.buffer.write(line.encode("utf-8"))
        else:
            out.write(line)
        out.flush()
    except (OSError, ValueError, AttributeError):
        pass


def _progress(text):
    _write_line("@@progress " + " ".join(str(text).split()))


def _blob():
    global _values
    if _values is None:
        _values = array.array("f")
        with open(os.path.join(work, "results.bin"), "rb") as f:
            _values.frombytes(f.read())
        if sys.byteorder != "little":
            _values.byteswap()
    return _values


def _need_results():
    if _results is None:
        raise RuntimeError("there are no results; run an analysis first")
    return _results


def _var(name, names, what):
    key = str(name).strip().lower().replace(" ", "")
    if key not in names:
        raise ValueError(
            f'unknown {what} result "{name}" ({", ".join(n.title() for n in names)})'
        )
    return names.index(key)


def _row(ids, id, what):
    try:
        return ids.index(str(id))
    except ValueError:
        raise KeyError(f'no results for {what} "{id}"') from None


def _check_period(p):
    n = _need_results()["periods"]
    if not 0 <= p < n:
        raise IndexError(f"period {p} is out of range (0 to {n - 1})")


def _node_at(p, v, r):
    res = _results
    nn, nl = len(res["node_ids"]), len(res["link_ids"])
    per = 4 * nn + 8 * nl
    return _blob()[p * per + v * nn + r]


def _link_at(p, v, r):
    res = _results
    nn, nl = len(res["node_ids"]), len(res["link_ids"])
    per = 4 * nn + 8 * nl
    return _blob()[p * per + 4 * nn + v * nl + r]


wepanet = types.ModuleType("wepanet")
wepanet.__doc__ = """WEPANET's project, its results, and what a plugin shows.

Examples (type them in a cell of the Python Notebook and press Shift+Enter;
the notebook's Examples menu adds them too):

    n = wepanet.node_ids()[0]                 # the first node's ID
    len(wepanet.node_ids())                   # how many nodes
    wepanet.node_type(n)                      # Junction, Reservoir or Tank
    wepanet.has_results()                     # True after an analysis
    wepanet.node_result(n, "Pressure", 0)     # a result at period 0
    wepanet.node_series(n, "Pressure")        # a result at every period
    wepanet.select_node(n)                    # selects it on the map
    wepanet.table("Types", ["Node", "Type"],
                  [[i, wepanet.node_type(i)] for i in wepanet.node_ids()])

Node results: Demand, Head, Pressure, Quality.
Link results: Flow, Velocity, Headloss, Quality, Status, Setting, Reaction,
Friction.
wepanet.inp_file is the project as an EPANET input file, e.g. for EPyT:

    from epyt import epanet
    d = epanet(wepanet.inp_file)

help(wepanet.node_result) shows one function."""
wepanet.in_browser = sys.platform == "emscripten"


def load(folder):
    """Reads the project and its results from WEPANET's folder."""
    global work, request, _results, _values
    work = os.path.abspath(folder)
    with open(os.path.join(work, "request.json"), encoding="utf-8") as f:
        request = json.load(f)
    _results = request.get("results")
    _values = None
    wepanet.work_dir = work
    #: The project as an EPANET input file, e.g. for EPyT: epanet(wepanet.inp_file).
    wepanet.inp_file = os.path.join(work, "network.inp")
    wepanet.item = request.get("item")
    #: The plugin's own folder: the other files of its ZIP (modules, which it
    #: can import, translations, …).
    wepanet.plugin_dir = os.path.join(work, "plugin")
    os.makedirs(wepanet.plugin_dir, exist_ok=True)


def _text(*args):
    return " ".join(str(a) for a in args)


def message(*args):
    """Shows a message when the plugin is over (in the notebook: under the cell)."""
    response["messages"].append(_text(*args))


def status(*args):
    """Puts a line on the status bar."""
    response["status"] = _text(*args)


def progress(*args):
    """Shows how far the plugin has got, while it runs."""
    _progress(_text(*args))


def table(title, header, rows):
    """Shows a table: a title, the column names and the rows."""
    response["table"] = {
        "title": str(title),
        "rows": [[str(h) for h in header]] + [[_cell(v) for v in r] for r in rows],
    }


def _cell(v):
    if isinstance(v, float):
        return f"{v:.6g}"
    return str(v)


def select_node(id):
    """Selects a node on the map."""
    response["select"] = {"node": str(id)}


def select_link(id):
    """Selects a link on the map."""
    response["select"] = {"link": str(id)}


def set_network(inp):
    """Replaces the project with an EPANET input file (its path, or its
    text), e.g. one saved by EPyT's saveInputFile. Undo takes it back."""
    out = os.path.join(work, "network_out.inp")
    if "\n" in str(inp):
        with open(out, "w", encoding="utf-8") as f:
            f.write(str(inp))
    else:
        shutil.copyfile(os.path.abspath(str(inp)), out)


def node_ids():
    """The IDs of the project's nodes."""
    return list(request["node_ids"])


def link_ids():
    """The IDs of the project's links."""
    return list(request["link_ids"])


def node_type(id):
    """Junction, Reservoir or Tank."""
    return request["node_types"][request["node_ids"].index(str(id))]


def link_type(id):
    """Pipe, Pump or Valve."""
    return request["link_types"][request["link_ids"].index(str(id))]


def flow_units():
    """The project's flow units, e.g. GPM or LPS."""
    return request["flow_units"]


def has_results():
    """True after an analysis of the project as it is."""
    return _results is not None


def periods():
    """The number of reporting periods of the results."""
    return _results["periods"] if _results else 0


def current_period():
    """The period shown on the map."""
    return request.get("period", 0)


def period_hours(p):
    """The time of a reporting period, in hours."""
    _check_period(p)
    return _results["times"][p] / 3600.0


def node_result(id, var, p):
    """A node's Demand, Head, Pressure or Quality at a period."""
    _check_period(p)
    v = _var(var, NODE_VARS, "node")
    return _node_at(p, v, _row(_results["node_ids"], id, "node"))


def link_result(id, var, p):
    """A link's Flow, Velocity, Headloss, Quality, Status, Setting,
    Reaction or Friction at a period."""
    _check_period(p)
    v = _var(var, LINK_VARS, "link")
    return _link_at(p, v, _row(_results["link_ids"], id, "link"))


def node_series(id, var):
    """A node's result at every period."""
    res = _need_results()
    v = _var(var, NODE_VARS, "node")
    r = _row(res["node_ids"], id, "node")
    return [_node_at(p, v, r) for p in range(res["periods"])]


def link_series(id, var):
    """A link's result at every period."""
    res = _need_results()
    v = _var(var, LINK_VARS, "link")
    r = _row(res["link_ids"], id, "link")
    return [_link_at(p, v, r) for p in range(res["periods"])]


for _f in [
    message,
    status,
    progress,
    table,
    select_node,
    select_link,
    set_network,
    node_ids,
    link_ids,
    node_type,
    link_type,
    flow_units,
    has_results,
    periods,
    current_period,
    period_hours,
    node_result,
    link_result,
    node_series,
    link_series,
]:
    setattr(wepanet, _f.__name__, _f)

# ----- plugins ----------------------------------------------------------------


def _plugin_traceback(e):
    """The error, with the plugin's own lines only."""
    tb = e.__traceback__
    while tb is not None and tb.tb_frame.f_code.co_filename != request["file"]:
        tb = tb.tb_next
    lines = traceback.format_exception(type(e), e, tb)
    return "".join(lines).strip()


def run_plugin(folder):
    """Runs plugin.py's run(item); answers in response.json."""
    load(folder)
    os.chdir(work)
    sys.modules["wepanet"] = wepanet
    sys.path.insert(0, wepanet.plugin_dir)
    _new_response()
    # Warnings (e.g. a library missing an optional package) are shown only
    # with an error, which they may explain.
    warned = []
    show = warnings.showwarning
    warnings.showwarning = lambda message, category, *_: warned.append(
        f"{category.__name__}: {message}"
    )
    printed = io.StringIO()
    stdout, stderr = sys.stdout, sys.stderr
    sys.stdout = sys.stderr = printed
    try:
        with open("plugin.py", encoding="utf-8") as f:
            source = f.read()
        scope = {"__name__": "wepanet_plugin", "__file__": request["file"]}
        # tracebacks show the plugin's lines
        linecache.cache[request["file"]] = (
            len(source),
            None,
            source.splitlines(True),
            request["file"],
        )
        exec(compile(source, request["file"], "exec"), scope)
        item = request.get("item")
        if item is not None:
            run = scope.get("run")
            if not callable(run):
                raise RuntimeError("the plugin has no def run(item)")
            run(item)
    except SystemExit:
        pass
    except ImportError as e:
        hint = ""
        if e.name and not wepanet.in_browser:
            hint = (
                f"\n\nInstall it for the Python WEPANET runs ({sys.executable}):\n"
                f"    {os.path.basename(sys.executable)} -m pip install {e.name}"
            )
        response["error"] = _plugin_traceback(e) + hint
    except BaseException as e:  # noqa: BLE001 - every error is the plugin's to report
        response["error"] = _plugin_traceback(e)
    finally:
        sys.stdout, sys.stderr = stdout, stderr
        warnings.showwarning = show
        if wepanet.plugin_dir in sys.path:
            sys.path.remove(wepanet.plugin_dir)
        # Pyodide keeps its modules from run to run: forget the plugin's own
        for name, module in list(sys.modules.items()):
            file = getattr(module, "__file__", None) or ""
            if os.path.abspath(file).startswith(wepanet.plugin_dir + os.sep):
                del sys.modules[name]
    if response["error"] and warned:
        response["error"] += "\n\nWarnings:\n" + "\n".join(dict.fromkeys(warned))
    said = printed.getvalue().strip()
    if said:
        response["messages"].insert(0, said)
    with open("response.json", "w", encoding="utf-8") as f:
        json.dump(response, f)


# ----- the Python Notebook ------------------------------------------------------

_cells = None
_cells_dir = None
_runs = 0
_PIP = re.compile(r"^(\s*)[%!]?pip install\s+(\S.*?)\s*$")
_EPYT = re.compile(r"^\s*(from|import)\s+epyt\b", re.M)
_PLOT = re.compile(r"plot|matplotlib")
# the packages of the modules whose names differ
_PACKAGES = {"PIL": "pillow", "cv2": "opencv-python", "sklearn": "scikit-learn", "yaml": "pyyaml"}
#: What the cells' EPyT projects were, by object, when a cell last saw them.
_epyt_seen = {}

# figures are drawn into PNGs, shown under their cell, not in windows
os.environ.setdefault("MPLBACKEND", "Agg")


def _help(*args, **kwargs):
    """help(object) shows the documentation of an object: help(wepanet) for
    the project and its results, with examples; help(wepanet.node_result)
    for one function."""
    if not args and not kwargs:
        print(_help.__doc__)
        return
    import pydoc

    print(pydoc.render_doc(*args, renderer=pydoc.plaintext, **kwargs))


def _no_input(*_):
    raise RuntimeError("input() is not available in the notebook")


def _pip_install(packages):
    """`%pip install` in a cell (in the browser, WEPANET installs the
    packages before the cell reaches Python)."""
    if wepanet.in_browser:
        print("Installed " + ", ".join(packages))
        return
    import subprocess

    r = subprocess.run(
        [sys.executable, "-m", "pip", "install", *packages],
        capture_output=True,
        text=True,
        stdin=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    print((r.stdout + r.stderr).strip())
    importlib.invalidate_caches()


def _exec_cell(source, filename):
    """Runs a cell: its `%pip install` lines install. Returns the value of
    its last line, when it is an expression (and the cell does not end in ;)."""
    lines = []
    for line in source.splitlines():
        m = _PIP.match(line)
        lines.append(f"{m[1]}_pip_install({m[2].split()!r})" if m else line)
    src = "\n".join(lines)
    linecache.cache[filename] = (len(src), None, src.splitlines(True), filename)
    tree = ast.parse(src, filename, "exec")
    last = None
    if tree.body and isinstance(tree.body[-1], ast.Expr) and not src.rstrip().endswith(";"):
        last = ast.Expression(tree.body.pop().value)
    exec(compile(tree, filename, "exec"), _cells)
    if last is None:
        return None
    value = eval(compile(last, filename, "eval"), _cells)
    if value is not None:
        _cells["_"] = value
    return value


def _plots_ready():
    """Matplotlib for a cell that plots: installed on the desktop when it is
    missing, and given to EPyT when EPyT was imported without it."""
    if importlib.util.find_spec("matplotlib") is None:
        print("Installing matplotlib…")
        _pip_install(["matplotlib"])
    try:
        import matplotlib
        import matplotlib.pyplot as plt
        from matplotlib import cm
    except ImportError:
        return
    # the figures are shown under the cell: plt.show() has nothing to do
    plt.show = lambda *args, **kwargs: None
    m = sys.modules.get("epyt.epanet")
    if m is not None and not hasattr(m, "plt"):
        m.mpl, m.plt, m.cm = matplotlib, plt, cm


def _figures(dpi):
    """The figures the cell drew, as PNG in base64 at `dpi` (then closed)."""
    plt = sys.modules.get("matplotlib.pyplot")
    if plt is None:
        return []
    import base64

    images = []
    for n in plt.get_fignums():
        buf = io.BytesIO()
        # Matplotlib's own deprecation warnings, while it draws
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            plt.figure(n).savefig(buf, format="png", dpi=dpi, bbox_inches="tight")
        images.append(base64.b64encode(buf.getvalue()).decode("ascii"))
    plt.close("all")
    return images


def _epyt_changes():
    """The network of an EPyT project of the cells that a cell changed (it
    goes to WEPANET's map), or None. Only projects opened from WEPANET's
    (`epanet(wepanet.inp_file)`) count: a notebook may open other networks
    too (EPyT's examples open their own). EPyT projects opened without
    `ph=True` share one EPANET project, the last one opened, so only that
    one counts of them; and the cell that opens one changes nothing. A
    project is compared with what it was when a cell last saw it."""
    objs = {
        id(o): (n, o)
        for n, o in list(_cells.items())
        if type(o).__name__ == "epanet" and type(o).__module__.startswith("epyt")
    }

    def shared(o):
        return getattr(getattr(o, "api", None), "_ph", None) is None

    opened = False
    for k, (_, o) in objs.items():
        if k not in _epyt_seen:
            _epyt_seen[k] = None
            opened = opened or shared(o)
    # in the order they were first seen: the last shared one holds EPANET
    live = [k for k in _epyt_seen if k in objs and shared(objs[k][1])][-1:]
    project = os.path.normcase(os.path.abspath(wepanet.inp_file))
    changed = None
    for k, (name, obj) in objs.items():
        file = getattr(obj, "InputFile", None)
        if not file or os.path.normcase(os.path.abspath(file)) != project:
            continue
        if shared(obj) and [k] != live:
            continue
        path = os.path.join(work, f"_epyt_{name}.inp")
        quiet = io.StringIO()
        stdout = sys.stdout
        sys.stdout = quiet
        try:
            obj.saveInputFile(path)
            with open(path, encoding="utf-8", errors="replace") as f:
                text = f.read()
        except Exception:
            continue
        finally:
            sys.stdout = stdout
        before = _epyt_seen[k]
        now = _inp_content(text)
        _epyt_seen[k] = now
        if before is not None and before != now and not opened:
            changed = text
    return changed


def _inp_content(text):
    """An input file without what an analysis changes in what EPANET saves:
    the comments and the type of a curve (GENERIC becomes PUMP)."""
    out = []
    curves = False
    for line in text.splitlines():
        line = line.split(";", 1)[0].strip()
        if not line:
            continue
        if line.startswith("["):
            curves = line.upper().startswith("[CURVES]")
        elif curves:
            line = " ".join(line.split()[:3])
        out.append(line)
    return "\n".join(out)


def run_cell(folder, source, reload, fresh=False, dpi=192):
    """Runs a cell of the Python Notebook. Returns JSON: what it printed, the
    value of its last line, the figures it drew, whether it stopped with an
    error, what it asked WEPANET to show, and the project it left (or null):
    from `wepanet.set_network`, or an EPyT project it changed."""
    global _cells, _cells_dir, _runs
    if _cells is None or fresh:
        _cells = {"__name__": "__main__", "wepanet": wepanet, "help": _help, "input": _no_input}
        _cells["_pip_install"] = _pip_install
        _epyt_seen.clear()
        _runs = 0
        # EPyT's Excel files are left out in the browser; figures are shown
        # under the cell, not in a window
        warnings.filterwarnings("ignore", message="(matplotlib|pandas) is not installed")
        warnings.filterwarnings("ignore", message=".*non-interactive.*")
    if reload or _cells_dir != folder:
        _cells_dir = folder
        load(folder)
    # in the browser a plugin may have run in between, elsewhere
    os.chdir(os.path.abspath(folder))
    sys.modules["wepanet"] = wepanet
    out_file = os.path.join(work, "network_out.inp")
    if os.path.exists(out_file):
        os.remove(out_file)
    _new_response()
    _runs += 1
    printed = io.StringIO()
    stdout, stderr = sys.stdout, sys.stderr
    sys.stdout = sys.stderr = printed
    error = False
    result = None
    images = []
    network = None
    try:
        # EPyT (with what it needs: an epyt installed without numpy cannot
        # open a network), the first time a cell imports it
        needs = ("epyt", "numpy", "cffi")
        if _EPYT.search(source) and any(importlib.util.find_spec(m) is None for m in needs):
            print("Installing epyt…")
            _pip_install(["epyt"])
        if _PLOT.search(source):
            _plots_ready()
        value = _exec_cell(source, f"<cell {_runs}>")
        if value is not None:
            result = repr(value)
    except SystemExit:
        print("The notebook stays open: close it with its Close button.")
    except BaseException as e:
        error = True
        # from the cell's own lines on
        tb = e.__traceback__
        while tb is not None and not tb.tb_frame.f_code.co_filename.startswith("<cell"):
            tb = tb.tb_next
        print("".join(traceback.format_exception(type(e), e, tb)), end="")
        if isinstance(e, ModuleNotFoundError) and e.name:
            print(f"Install it with a cell: %pip install {_PACKAGES.get(e.name, e.name)}")
    finally:
        try:
            images = _figures(dpi)
            network = _epyt_changes()
        except Exception:
            pass
        sys.stdout, sys.stderr = stdout, stderr
    if os.path.exists(out_file):
        with open(out_file, encoding="utf-8") as f:
            network = f.read()
    output = printed.getvalue()
    if response["messages"]:
        output += "\n".join(response["messages"]) + "\n"
    response["messages"] = []
    return json.dumps(
        {
            "output": output,
            "result": result,
            "images": images,
            "error": error,
            "response": response,
            "network": network,
        }
    )


def _notebook_loop(folder):
    for raw in sys.stdin.buffer:
        try:
            msg = json.loads(raw.decode("utf-8"))
        except ValueError:
            continue
        _write_line(
            "@@cell "
            + run_cell(
                folder,
                msg.get("cell", ""),
                msg.get("reload", False),
                dpi=msg.get("dpi", 192),
            )
        )


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "--notebook":
        _notebook_loop(sys.argv[2])
    else:
        run_plugin(sys.argv[1] if len(sys.argv) > 1 else ".")
