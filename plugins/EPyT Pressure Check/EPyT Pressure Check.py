# EPyT Pressure Check — a Python plugin that runs its own analysis of the
# project with EPyT (https://github.com/OpenWaterAnalytics/EPyT), the EPANET
# Python toolkit, and shows the junctions with the lowest pressures.
#
# A Python plugin declares its NAME, DESCRIPTION, MENU (its commands) and the
# PyPI packages it needs (REQUIREMENTS), and defines run(item), called with
# the command chosen. `wepanet` gives the project (wepanet.inp_file, an EPANET
# input file) and shows what the plugin found. In the browser WEPANET installs
# the REQUIREMENTS itself; on the desktop: python -m pip install epyt

NAME = "EPyT Pressure Check"
DESCRIPTION = "Lowest junction pressures, from an EPyT analysis of the project"
MENU = ["Lowest Pressures", "Pressure Statistics"]
REQUIREMENTS = ["epyt"]

import numpy as np
import wepanet
from epyt import epanet


def run(item):
    if not wepanet.node_ids():
        wepanet.message("Open a network first (File > Open, or a sample project).")
        return
    if not any(wepanet.node_type(n) == "Junction" for n in wepanet.node_ids()):
        wepanet.message("The network has no junctions.")
        return
    wepanet.progress("Loading the project in EPyT…")
    d = epanet(wepanet.inp_file, display_msg=False, display_warnings=False)
    try:
        wepanet.progress("Running the hydraulics…")
        h = d.getComputedHydraulicTimeSeries()
        ids = d.getNodeNameID()
        junctions = [k - 1 for k in d.getNodeJunctionIndex()]
        pressure = np.asarray(h.Pressure)[:, junctions]  # time x junction
        hours = np.asarray(h.Time) / 3600.0
        if item == "Lowest Pressures":
            lowest(ids, junctions, pressure, hours)
        else:
            statistics(ids, junctions, pressure)
    finally:
        d.unload()


def lowest(ids, junctions, pressure, hours):
    worst = pressure.argmin(axis=0)
    rows = sorted(
        (
            [ids[j], round(float(pressure[worst[k], k]), 2), round(float(hours[worst[k]]), 2)]
            for k, j in enumerate(junctions)
        ),
        key=lambda r: r[1],
    )[:20]
    wepanet.select_node(rows[0][0])
    wepanet.table(
        "The 20 lowest junction pressures (EPyT)",
        ["Junction", "Lowest pressure", "At hour"],
        rows,
    )


def statistics(ids, junctions, pressure):
    rows = [
        [
            ids[j],
            round(float(pressure[:, k].min()), 2),
            round(float(pressure[:, k].mean()), 2),
            round(float(pressure[:, k].max()), 2),
        ]
        for k, j in enumerate(junctions)
    ]
    wepanet.table(
        "Junction pressures over the simulation (EPyT)",
        ["Junction", "Min", "Mean", "Max"],
        rows,
    )
    print(f"Pressures of {len(rows)} junctions over {pressure.shape[0]} time steps.")
