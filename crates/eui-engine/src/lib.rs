//! Simulation engine bridge.
//!
//! The contract is deliberately tiny and identical on every platform:
//! INP text goes in, EPANET's binary output file and report come out.
//! Parsing the results is done in pure Rust by `eui_core::results`.
//!
//! * native – the vendored OWA EPANET C sources are compiled into this crate.
//! * wasm32 – the same C sources compiled with Emscripten run in a Web Worker
//!   (see `web-engine/`); this crate talks to it through `window.euiEngine`.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{engine_version, run_blocking};

/// What a finished run returns.
#[derive(Clone, Debug, Default)]
pub struct RunOutput {
    /// EPANET return code: 0 = ok, 1..99 = warnings, >= 100 = error.
    pub code: i32,
    /// Plain-text status report written by EPANET.
    pub report: String,
    /// Contents of the binary output file (empty on fatal errors).
    pub out: Vec<u8>,
    /// Energy balance sums added up at every hydraulic time step, as
    /// EPANET-UI does (see `eui_core::analysis::EnergySums`).
    pub energy: Option<[f64; ENERGY_SIZE]>,
    /// Results EPANET's output file lacks, for each reporting period (see
    /// `eui_core::results::ExtraResults`); empty if not available.
    pub extra: Vec<u8>,
}

/// Number of energy balance sums returned by the engine.
pub const ENERGY_SIZE: usize = 9;

impl RunOutput {
    pub fn is_error(&self) -> bool {
        self.code >= 100
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("the simulation engine is not available: {0}")]
    Unavailable(String),
    #[error("the run was cancelled")]
    Cancelled,
}

/// What a running simulation is solving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Hydraulics,
    Quality,
}

/// How far a simulation has got: the stage and the simulation hour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub hour: u32,
}

impl Progress {
    /// From the engine's stage code (1 hydraulics, 2 quality) and hour.
    #[allow(dead_code)]
    fn new(stage: i32, hour: i64) -> Progress {
        Progress {
            stage: if stage == 2 {
                Stage::Quality
            } else {
                Stage::Hydraulics
            },
            hour: hour.max(0) as u32,
        }
    }
}

/// A simulation running in the background (a thread natively, a Web Worker in
/// the browser). Poll it from the UI loop.
pub struct RunHandle {
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<RunOutput, EngineError>>,
    #[cfg(not(target_arch = "wasm32"))]
    progress: std::sync::Arc<std::sync::Mutex<Option<Progress>>>,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_arch = "wasm32")]
    slot: std::rc::Rc<std::cell::RefCell<Option<Result<RunOutput, EngineError>>>>,
    #[cfg(target_arch = "wasm32")]
    progress: std::rc::Rc<std::cell::Cell<Option<Progress>>>,
}

impl RunHandle {
    /// Returns the result once the run has finished.
    pub fn try_take(&mut self) -> Option<Result<RunOutput, EngineError>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                    EngineError::Unavailable("the simulation thread stopped unexpectedly".into()),
                )),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.slot.borrow_mut().take()
        }
    }

    /// The stage and hour last reported by the engine.
    pub fn progress(&self) -> Option<Progress> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.progress.lock().ok().and_then(|p| *p)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.progress.get()
        }
    }

    /// Asks the run to stop; it then finishes with [`EngineError::Cancelled`].
    pub fn cancel(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        #[cfg(target_arch = "wasm32")]
        web::cancel();
    }
}

/// Starts a simulation of `inp` in the background. `repaint` is called as it
/// progresses and when it finishes (use it to request a UI repaint).
pub fn spawn_run(inp: String, repaint: impl Fn() + Send + 'static) -> RunHandle {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};
        let (tx, rx) = std::sync::mpsc::channel();
        let progress = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let (p, c) = (progress.clone(), cancel.clone());
        std::thread::spawn(move || {
            let result = native::run_with_progress(&inp, &mut |pr| {
                if let Ok(mut slot) = p.lock() {
                    *slot = Some(pr);
                }
                repaint();
                c.load(Ordering::Relaxed)
            });
            let _ = tx.send(result);
            repaint();
        });
        RunHandle {
            rx,
            progress,
            cancel,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let slot = Rc::new(RefCell::new(None));
        let progress = Rc::new(Cell::new(None));
        let (s, p) = (slot.clone(), progress.clone());
        let repaint = Rc::new(repaint);
        let r = repaint.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = web::run(&inp, move |pr| {
                p.set(Some(pr));
                r();
            })
            .await;
            *s.borrow_mut() = Some(result);
            repaint();
        });
        RunHandle { slot, progress }
    }
}

/// A change of units or head loss formula for [`spawn_convert`], as EPANET's
/// codes (negative = keep): flow units 0 CFS … 10 CMS, pressure units 0 psi,
/// 1 kPa, 2 m, 3 bar, 4 ft, head loss 0 H-W, 1 D-W, 2 C-M.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitChange {
    pub flow_units: i32,
    pub pressure_units: i32,
    pub headloss: i32,
}

/// A unit conversion running in the background.
pub struct ConvertHandle {
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<String, EngineError>>,
    #[cfg(target_arch = "wasm32")]
    slot: std::rc::Rc<std::cell::RefCell<Option<Result<String, EngineError>>>>,
}

impl ConvertHandle {
    /// The converted INP text once it is ready.
    pub fn try_take(&mut self) -> Option<Result<String, EngineError>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.rx.try_recv().ok()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.slot.borrow_mut().take()
        }
    }
}

/// Converts `inp` to other units with EPANET itself (EN_setflowunits converts
/// all the data), in the background.
pub fn spawn_convert(
    inp: String,
    change: UnitChange,
    repaint: impl Fn() + Send + 'static,
) -> ConvertHandle {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(native::convert_units(&inp, change));
            repaint();
        });
        ConvertHandle { rx }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let s = slot.clone();
        wasm_bindgen_futures::spawn_local(async move {
            *s.borrow_mut() = Some(web::convert(&inp, change).await);
            repaint();
        });
        ConvertHandle { slot }
    }
}
