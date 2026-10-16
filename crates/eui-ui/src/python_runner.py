"""WEPANET's Python: the runner of Python plugins and the Python Console.

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
    python python_runner.py --console <folder>   the Python Console: reads
                                                 {"line", "reload"} JSON lines,
                                                 answers "@@console {...}" lines

In the browser the module is loaded once and `run_plugin(folder)` and
`console_push(folder, line, reload)` are called.
"""

import array
import code
import io
import json
import linecache
import os
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
wepanet.__doc__ = "WEPANET's project, its results, and what a plugin shows."
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
    """Shows a message when the plugin is over (in the console: at once)."""
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


# ----- the Python Console -------------------------------------------------------

_console = None
_console_dir = None


def _help(*args, **kwargs):
    """help(object) shows the documentation of an object, e.g. help(wepanet)."""
    if not args and not kwargs:
        print(_help.__doc__)
        return
    import pydoc

    print(pydoc.render_doc(*args, renderer=pydoc.plaintext, **kwargs))


def _no_input(*_):
    raise RuntimeError("input() is not available in the console")


def _pip_install(packages):
    """`pip install` typed in the console (in the browser, WEPANET installs
    the packages before the line reaches Python)."""
    import subprocess

    r = subprocess.run(
        [sys.executable, "-m", "pip", "install", *packages],
        capture_output=True,
        text=True,
        stdin=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    print((r.stdout + r.stderr).strip())


def console_push(folder, line, reload):
    """Runs a line typed in the Python Console (a block ends with an empty
    line). Returns JSON: what it printed, whether a block goes on, what it
    asked WEPANET to show, and the project it left (or null)."""
    global _console, _console_dir
    if _console is None:
        _console = code.InteractiveConsole(
            {
                "__name__": "__console__",
                "wepanet": wepanet,
                "help": _help,
                "input": _no_input,
            },
            filename="<console>",
        )
    if reload or _console_dir != folder:
        _console_dir = folder
        load(folder)
    # in the browser a plugin may have run in between, elsewhere
    os.chdir(os.path.abspath(folder))
    sys.modules["wepanet"] = wepanet
    out_file = os.path.join(work, "network_out.inp")
    if os.path.exists(out_file):
        os.remove(out_file)
    _new_response()
    printed = io.StringIO()
    stdout, stderr = sys.stdout, sys.stderr
    sys.stdout = sys.stderr = printed
    try:
        pip = line.strip().removeprefix("%").split()
        if pip[:2] == ["pip", "install"] and len(pip) > 2 and not _console.buffer:
            more = False
            _pip_install(pip[2:])
        else:
            more = _console.push(line)
    except SystemExit:
        more = False
        print("The console stays open: close it with its Close button.")
    finally:
        sys.stdout, sys.stderr = stdout, stderr
    network = None
    if os.path.exists(out_file):
        with open(out_file, encoding="utf-8") as f:
            network = f.read()
    output = printed.getvalue()
    if response["messages"]:
        output += "\n".join(response["messages"]) + "\n"
    response["messages"] = []
    return json.dumps(
        {"output": output, "more": bool(more), "response": response, "network": network}
    )


def _console_loop(folder):
    for raw in sys.stdin.buffer:
        try:
            msg = json.loads(raw.decode("utf-8"))
        except ValueError:
            continue
        _write_line(
            "@@console " + console_push(folder, msg.get("line", ""), msg.get("reload", False))
        )


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "--console":
        _console_loop(sys.argv[2])
    else:
        run_plugin(sys.argv[1] if len(sys.argv) > 1 else ".")
