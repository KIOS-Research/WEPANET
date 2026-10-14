/*
 * Thin wrapper around the OWA EPANET toolkit used by EPANET-UI.
 *
 * The same file is compiled natively (via the eui-engine build script) and
 * with Emscripten for the web app (see web-engine/build.sh), so both
 * platforms run the identical engine through the identical entry point.
 */
#include <math.h>
#include <stdio.h>
#include <string.h>
#include "epanet2_2.h"
#include "types.h"

/* Energy balance sums, |Q*H*dt| in project units, as accumulated by
 * EPANET-UI's energycalc.pas; EUI_TSUM is the time they cover (s). */
enum {
    EUI_INFLOWS, EUI_PUMPING, EUI_TANKOUT, EUI_DEMANDS, EUI_LEAKAGE,
    EUI_FRICTION, EUI_TANKIN, EUI_MINUSE, EUI_TSUM, EUI_ENERGY_SIZE
};

#define EUI_SEC_PER_DAY 86400
#define EUI_M_PER_FT 0.3048

/* The service pressure of the demand model as a head in project units
 * (energycalc.pas: Start). */
static double required_head(EN_Project ph)
{
    int model = 0;
    double pmin = 0, preq = 0, pexp = 0, punits = 0, spgrav = 1, flowunits = 0;
    EN_getdemandmodel(ph, &model, &pmin, &preq, &pexp);
    EN_getoption(ph, EN_PRESS_UNITS, &punits);
    EN_getoption(ph, EN_SP_GRAVITY, &spgrav);
    EN_getflowunits(ph, &model);
    flowunits = model;
    switch ((int)punits) {
    case EN_PSI:    preq /= 0.4333; break;
    case EN_KPA:    preq *= 0.334553; break;
    case EN_METERS: preq /= EUI_M_PER_FT; break;
    case EN_BAR:    preq *= 33.4553; break;
    default:        break; /* EN_FEET */
    }
    if ((int)flowunits >= EN_LPS) preq *= EUI_M_PER_FT;
    if (spgrav > 0) preq /= spgrav;
    return preq;
}

/* Adds the energy of the current hydraulic solution over dt seconds
 * (energycalc.pas: UpdateNodeEnergy and UpdateLinkEnergy). */
static void add_energy(EN_Project ph, double preq, double dt, double *e)
{
    int nnodes = 0, nlinks = 0, i;
    double hmin = 0, h;
    EN_getcount(ph, EN_NODECOUNT, &nnodes);
    EN_getcount(ph, EN_LINKCOUNT, &nlinks);
    for (i = 1; i <= nnodes; i++) {
        EN_getnodevalue(ph, i, EN_HEAD, &h);
        if (h < hmin) hmin = h;
    }
    for (i = 1; i <= nnodes; i++) {
        int type = 0;
        double el = 0, qd = 0, ql = 0, en;
        EN_getnodetype(ph, i, &type);
        EN_getnodevalue(ph, i, EN_ELEVATION, &el);
        EN_getnodevalue(ph, i, EN_HEAD, &h);
        EN_getnodevalue(ph, i, EN_DEMAND, &qd);
        EN_getnodevalue(ph, i, EN_LEAKAGEFLOW, &ql);
        qd -= ql;          /* consumer demand without leakage */
        h -= hmin;         /* no negative heads */
        en = fabs(qd * h * dt);
        if (type == EN_JUNCTION) {
            e[EUI_MINUSE] += fabs(qd * (el + preq - hmin) * dt);
            if (qd > 0) e[EUI_DEMANDS] += en;
            else e[EUI_INFLOWS] += en;
            if (ql > 0) e[EUI_LEAKAGE] += ql * h * dt;
            else e[EUI_INFLOWS] -= ql * h * dt;
        } else if (type == EN_RESERVOIR) {
            if (qd < 0) e[EUI_INFLOWS] += en;
            else e[EUI_DEMANDS] += en;
        } else {
            if (qd > 0) e[EUI_TANKIN] += en;
            else e[EUI_TANKOUT] += en;
        }
    }
    for (i = 1; i <= nlinks; i++) {
        int n1 = 0, n2 = 0, type = 0;
        double q = 0, h1 = 0, h2 = 0, pw;
        if (EN_getlinknodes(ph, i, &n1, &n2)) continue;
        EN_getlinktype(ph, i, &type);
        EN_getlinkvalue(ph, i, EN_FLOW, &q);
        EN_getnodevalue(ph, n1, EN_HEAD, &h1);
        EN_getnodevalue(ph, n2, EN_HEAD, &h2);
        pw = fabs((h1 - h2) * q) * dt;
        if (type == EN_PUMP) e[EUI_PUMPING] += pw;
        else e[EUI_FRICTION] += pw;
    }
    e[EUI_TSUM] += dt;
}

/* Progress of a run: the stage and the simulation hour being solved.
 * A non-zero return asks the run to stop. */
#define EUI_HYDRAULICS 1
#define EUI_QUALITY 2
#define EUI_CANCELLED (-1)
typedef int (*eui_progress_fn)(int stage, long hour, void *ctx);

#ifdef __EMSCRIPTEN__
#include <emscripten.h>
/* In the browser the worker (engine-worker.js) relays progress to the page. */
EM_JS(void, eui_js_progress, (int stage, int hour), {
    if (typeof self !== 'undefined' && self.euiProgress) self.euiProgress(stage, hour);
});
#endif

/* Reports progress when the hour changes; returns non-zero to cancel. */
static int progress(eui_progress_fn cb, void *ctx, int stage, long t, long *last)
{
    long hour = t / 3600;
    if (hour == *last) return 0;
    *last = hour;
#ifdef __EMSCRIPTEN__
    eui_js_progress(stage, (int)hour);
#endif
    return cb ? cb(stage, hour, ctx) : 0;
}

/* Writes, for one reporting period, the results EPANET's output file lacks
 * (EPANET-UI's second output file): per node the demand deficit (% of the
 * full demand), emitter flow and leakage, then per link the leakage and the
 * pump energy, as 32-bit floats. */
static void save_extra(EN_Project p, FILE *f)
{
    int nnodes = 0, nlinks = 0, i;
    double v, full;
    float x;
    EN_getcount(p, EN_NODECOUNT, &nnodes);
    EN_getcount(p, EN_LINKCOUNT, &nlinks);
    for (i = 1; i <= nnodes; i++) {
        v = 0; full = 0;
        EN_getnodevalue(p, i, EN_DEMANDDEFICIT, &v);
        EN_getnodevalue(p, i, EN_FULLDEMAND, &full);
        x = (full > 0 && v > 0) ? (float)(v / full * 100.0) : 0.0f;
        fwrite(&x, sizeof x, 1, f);
    }
    for (i = 1; i <= nnodes; i++) {
        v = 0;
        EN_getnodevalue(p, i, EN_EMITTERFLOW, &v);
        x = (float)v;
        fwrite(&x, sizeof x, 1, f);
    }
    for (i = 1; i <= nnodes; i++) {
        v = 0;
        EN_getnodevalue(p, i, EN_LEAKAGEFLOW, &v);
        x = (float)v;
        fwrite(&x, sizeof x, 1, f);
    }
    for (i = 1; i <= nlinks; i++) {
        v = 0;
        EN_getlinkvalue(p, i, EN_LINK_LEAKAGE, &v);
        x = (float)v;
        fwrite(&x, sizeof x, 1, f);
    }
    for (i = 1; i <= nlinks; i++) {
        v = 0;
        EN_getlinkvalue(p, i, EN_ENERGY, &v);
        x = (float)v;
        fwrite(&x, sizeof x, 1, f);
    }
}

/* Runs a complete simulation: reads inpFile, writes the report to rptFile
 * and binary results to outFile. Returns the EPANET error/warning code
 * (0 = ok, < 100 = warning, >= 100 = error), or EUI_CANCELLED.
 *
 * This is EN_runproject with the loops of EN_solveH and EN_solveQ written
 * out, so that the energy balance can be added up at every hydraulic time
 * step as EPANET-UI does (into energy, EUI_ENERGY_SIZE doubles, if not NULL),
 * the results missing from the output file saved at each reporting time
 * (into extraFile, see save_extra, if not NULL) and progress reported to cb
 * (which may be NULL). */
int eui_run_progress(const char *inpFile, const char *rptFile, const char *outFile,
                     const char *extraFile, double *energy, eui_progress_fn cb, void *ctx)
{
    EN_Project p = NULL;
    FILE *extra = NULL;
    int errcode = 0, cancelled = 0, i;
    long t = 0, tstep = 0, last = -1, rstart = 0, rstep = 0;
    double preq = 0;
    double e[EUI_ENERGY_SIZE];
    for (i = 0; i < EUI_ENERGY_SIZE; i++) e[i] = 0;

    errcode = EN_createproject(&p);
    if (errcode) return errcode;

    ERRCODE(EN_open(p, inpFile, rptFile, outFile));
    if (errcode <= 100 && p->outfile.Hydflag != USE) {
        int herr = EN_openH(p);
        preq = required_head(p);
        EN_gettimeparam(p, EN_REPORTSTART, &rstart);
        EN_gettimeparam(p, EN_REPORTSTEP, &rstep);
        if (extraFile) extra = fopen(extraFile, "wb");
        if (!herr) {
            herr = EN_initH(p, EN_SAVE);
            if (!herr) do {
                if (progress(cb, ctx, EUI_HYDRAULICS, t, &last)) { cancelled = 1; break; }
                tstep = 0;
                herr = (herr > 100) ? herr : EN_runH(p, &t);
                /* the reporting periods of the output file */
                if (extra && herr <= 100 && t >= rstart
                    && (rstep <= 0 || (t - rstart) % rstep == 0))
                    save_extra(p, extra);
                herr = (herr > 100) ? herr : EN_nextH(p, &tstep);
                /* energycalc.pas: a snapshot run counts as a whole day */
                if (tstep > 0) add_energy(p, preq, (double)tstep, e);
                else if (t == 0) add_energy(p, preq, EUI_SEC_PER_DAY, e);
            } while (tstep > 0);
        }
        EN_closeH(p);
        if (extra) fclose(extra);
        herr = MAX(herr, p->Warnflag);
        ERRCODE(herr);
    }
    if (!cancelled && errcode <= 100) {
        /* EN_solveQ */
        int qerr = EN_openQ(p);
        t = 0;
        last = -1;
        if (!qerr) {
            qerr = EN_initQ(p, EN_SAVE);
            if (!qerr) do {
                if (p->quality.Qualflag
                    && progress(cb, ctx, EUI_QUALITY, t, &last)) { cancelled = 1; break; }
                tstep = 0;
                qerr = (qerr > 100) ? qerr : EN_runQ(p, &t);
                qerr = (qerr > 100) ? qerr : EN_nextQ(p, &tstep);
            } while (tstep > 0);
        }
        EN_closeQ(p);
        ERRCODE(qerr);
    }
    if (!cancelled) ERRCODE(EN_report(p));
    EN_close(p);
    if (p->Warnflag) errcode = MAX(errcode, p->Warnflag);
    EN_deleteproject(p);
    if (energy) memcpy(energy, e, sizeof e);
    return cancelled ? EUI_CANCELLED : errcode;
}

int eui_run_energy(const char *inpFile, const char *rptFile, const char *outFile,
                   double *energy)
{
    return eui_run_progress(inpFile, rptFile, outFile, NULL, energy, NULL, NULL);
}

int eui_run(const char *inpFile, const char *rptFile, const char *outFile)
{
    return eui_run_energy(inpFile, rptFile, outFile, NULL);
}

/* Changes a project's units or head loss formula the way EPANET does it
 * (EN_setflowunits converts all the data, also between US and SI units) and
 * writes the result to outInp, as EPANET-UI's Project Setup does. A negative
 * code leaves that setting as it is. Returns an EPANET error code. */
int eui_convert_units(const char *inpFile, const char *rptFile, const char *outInp,
                      int flowUnits, int pressUnits, int hlossModel)
{
    EN_Project p = NULL;
    int err = EN_createproject(&p);
    if (err) return err;
    err = EN_open(p, inpFile, rptFile, "");
    if (err <= 100 && flowUnits >= 0) err = MAX(err, EN_setflowunits(p, flowUnits));
    if (err <= 100 && pressUnits >= 0)
        err = MAX(err, EN_setoption(p, EN_PRESS_UNITS, pressUnits));
    if (err <= 100 && hlossModel >= 0)
        err = MAX(err, EN_setoption(p, EN_HEADLOSSFORM, hlossModel));
    if (err <= 100) err = MAX(err, EN_saveinpfile(p, outInp));
    EN_close(p);
    EN_deleteproject(p);
    return err;
}

/* Copies the text of an EPANET error code into msg (at least 256 bytes). */
void eui_error_message(int code, char *msg)
{
    msg[0] = '\0';
    EN_geterror(code, msg, 255);
}

int eui_version(void)
{
    int v = 0;
    EN_getversion(&v);
    return v;
}
