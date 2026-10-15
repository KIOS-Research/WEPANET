# Vendored sources

| Folder | Upstream | Revision | License |
|---|---|---|---|
| `epanet/` | https://github.com/OpenWaterAnalytics/EPANET (`dev`) | `66366b5a89340aded0f00a933e22c867466de98b` (2026-10-12, CODEVERSION 20306) | MIT |
| `epanet-msx/` | https://github.com/USEPA/EPANETMSX (`master`) | `85a303dca05859389eced63fabd5ea145c096333` (2025-03-27, EPANET-MSX 2.0.0) | MIT |

From EPANET only `src/`, `include/`, `example-networks/`, the license and docs
were kept. To update: replace the folder with a newer checkout and run
`cargo test`.

From EPANET-MSX only `src/solver/` (as `src/`), `include/epanetmsx.h`, the
example, the license and the notes were kept. A few lines were changed, each
marked `WEPANET`:

- `src/msxutils.c`: `MSXutils_getTempName` puts scratch files in the system's
  temporary directory instead of the current one.
- `src/msxout.c` includes `<stdlib.h>` on every platform, not only on macOS.
- `src/msxqual.c` and `src/msxtoolkit.c` declare `MSXerr_writeMathErrorMsg` and
  `MSXproj_close` as they are defined (returning nothing), which WebAssembly
  requires.

WEPANET drops the `COMPILER` option before a run, since it needs a C compiler
on the user's machine.
