//! Reader for EPANET's binary output file.
//!
//! The engine (native or WebAssembly) always produces this file, so a single
//! reader serves every platform. Layout (all little-endian 4-byte values):
//!
//! * prolog  – 15 ints, titles, file names, IDs, link topology, static props
//! * energy  – per pump: index + 6 reals, then the peak demand charge
//! * dynamic – per reporting period: 4 node vars × nodes, 8 link vars × links
//! * epilog  – 4 reaction rates, period count, warning flag, magic number

use crate::units;

const MAGIC: i32 = 516_114_521;
const ID_LEN: usize = 32; // MAXID + 1
const TITLE_LEN: usize = 80; // TITLELEN + 1
const FNAME_LEN: usize = 260; // MAXFNAME + 1
const EPILOG_LEN: usize = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeVar {
    Demand,
    Head,
    Pressure,
    Quality,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkVar {
    Flow,
    Velocity,
    Headloss,
    Quality,
    Status,
    Setting,
    Reaction,
    Friction,
}

impl NodeVar {
    pub const ALL: [NodeVar; 4] = [
        NodeVar::Demand,
        NodeVar::Head,
        NodeVar::Pressure,
        NodeVar::Quality,
    ];
    pub fn label(self) -> &'static str {
        match self {
            NodeVar::Demand => "Demand",
            NodeVar::Head => "Head",
            NodeVar::Pressure => "Pressure",
            NodeVar::Quality => "Quality",
        }
    }
}

impl LinkVar {
    /// Variables worth showing on a map (status/setting/friction are mostly diagnostic).
    pub const MAPPABLE: [LinkVar; 5] = [
        LinkVar::Flow,
        LinkVar::Velocity,
        LinkVar::Headloss,
        LinkVar::Quality,
        LinkVar::Reaction,
    ];
    pub fn label(self) -> &'static str {
        match self {
            LinkVar::Flow => "Flow",
            LinkVar::Velocity => "Velocity",
            LinkVar::Headloss => "Unit Headloss",
            LinkVar::Quality => "Quality",
            LinkVar::Status => "Status",
            LinkVar::Setting => "Setting",
            LinkVar::Reaction => "Reaction Rate",
            LinkVar::Friction => "Friction Factor",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PumpEnergy {
    pub link_index: usize,
    pub utilization_pct: f32,
    pub efficiency_pct: f32,
    pub kwh_per_volume: f32,
    pub avg_kw: f32,
    pub peak_kw: f32,
    pub cost_per_day: f32,
}

#[derive(Clone, Debug, Default)]
pub struct SimResults {
    pub node_ids: Vec<String>,
    pub link_ids: Vec<String>,
    pub n_periods: usize,
    /// Seconds.
    pub report_start: u32,
    pub report_step: u32,
    pub duration: u32,
    pub flow_units: usize,
    pub pressure_units: usize,
    /// 0 none, 1 chemical, 2 age, 3 trace.
    pub quality_flag: i32,
    pub chem_name: String,
    pub chem_units: String,
    pub pumps: Vec<PumpEnergy>,
    /// Peak energy demand charge.
    pub demand_charge: f32,
    pub warning: bool,
    nodes: Vec<f32>,
    links: Vec<f32>,
    /// Results the output file lacks, per period (see [`SimResults::set_extra`]).
    extra: Vec<f32>,
}

/// Node results that EPANET's output file lacks, saved by the engine at
/// each reporting period (EPANET-UI's second output file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeExtra {
    /// Demand deficit as a percent of the full demand.
    DemandDeficit,
    EmitterFlow,
    Leakage,
}

/// Link results that EPANET's output file lacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkExtra {
    Leakage,
    /// Pump energy use (kW).
    Energy,
}

#[derive(Debug, thiserror::Error)]
pub enum OutError {
    #[error("the output file is truncated or empty")]
    Truncated,
    #[error("not an EPANET output file")]
    BadMagic,
    #[error("the simulation produced no results")]
    NoResults,
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], OutError> {
        let s = self
            .buf
            .get(self.pos..self.pos + n)
            .ok_or(OutError::Truncated)?;
        self.pos += n;
        Ok(s)
    }
    fn i32(&mut self) -> Result<i32, OutError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, OutError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn str(&mut self, n: usize) -> Result<String, OutError> {
        let raw = self.take(n)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(n);
        Ok(String::from_utf8_lossy(&raw[..end]).trim().to_string())
    }
    fn skip(&mut self, n: usize) -> Result<(), OutError> {
        self.take(n).map(|_| ())
    }
    fn f32s(&mut self, n: usize, out: &mut Vec<f32>) -> Result<(), OutError> {
        let raw = self.take(n * 4)?;
        out.extend(
            raw.as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c)),
        );
        Ok(())
    }
}

impl SimResults {
    /// Attaches the engine's extra results: per period, 3 node values then 2
    /// link values per element, as 32-bit floats. Ignored if the size is off.
    pub fn set_extra(&mut self, bytes: &[u8]) {
        let (nn, nl) = (self.node_ids.len(), self.link_ids.len());
        if bytes.len() != self.n_periods * (3 * nn + 2 * nl) * 4 {
            self.extra.clear();
            return;
        }
        self.extra = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
    }

    pub fn has_extra(&self) -> bool {
        !self.extra.is_empty()
    }

    /// An extra node result at period `p` for result row `r`.
    pub fn node_extra(&self, p: usize, v: NodeExtra, r: usize) -> Option<f32> {
        let (nn, nl) = (self.node_ids.len(), self.link_ids.len());
        let k = v as usize;
        self.extra.get(p * (3 * nn + 2 * nl) + k * nn + r).copied()
    }

    /// An extra link result at period `p` for result row `r`.
    pub fn link_extra(&self, p: usize, v: LinkExtra, r: usize) -> Option<f32> {
        let (nn, nl) = (self.node_ids.len(), self.link_ids.len());
        let k = v as usize;
        self.extra
            .get(p * (3 * nn + 2 * nl) + 3 * nn + k * nl + r)
            .copied()
    }

    pub fn from_out(buf: &[u8]) -> Result<Self, OutError> {
        if buf.len() < 60 + EPILOG_LEN {
            return Err(OutError::Truncated);
        }
        let mut c = Cursor { buf, pos: 0 };
        let mut ints = [0i32; 15];
        for v in &mut ints {
            *v = c.i32()?;
        }
        if ints[0] != MAGIC {
            return Err(OutError::BadMagic);
        }
        let n_nodes = ints[2] as usize;
        let n_tanks = ints[3] as usize;
        let n_links = ints[4] as usize;
        let n_pumps = ints[5] as usize;

        // Epilog tells us the number of periods and whether the run finished.
        let mut e = Cursor {
            buf,
            pos: buf.len() - EPILOG_LEN,
        };
        e.skip(16)?;
        let n_periods = e.i32()?.max(0) as usize;
        let warning = e.i32()? != 0;
        if e.i32()? != MAGIC {
            return Err(OutError::Truncated);
        }
        if n_periods == 0 {
            return Err(OutError::NoResults);
        }

        c.skip(3 * TITLE_LEN + 2 * FNAME_LEN)?;
        let chem_name = c.str(ID_LEN)?;
        let chem_units = c.str(ID_LEN)?;
        let node_ids = (0..n_nodes)
            .map(|_| c.str(ID_LEN))
            .collect::<Result<Vec<_>, _>>()?;
        let link_ids = (0..n_links)
            .map(|_| c.str(ID_LEN))
            .collect::<Result<Vec<_>, _>>()?;
        // link N1, N2, type; tank node index and area; node elev; link length, diameter
        c.skip(4 * (3 * n_links + 2 * n_tanks + n_nodes + 2 * n_links))?;

        let mut pumps = Vec::with_capacity(n_pumps);
        for _ in 0..n_pumps {
            pumps.push(PumpEnergy {
                link_index: (c.i32()? - 1).max(0) as usize,
                utilization_pct: c.f32()?,
                efficiency_pct: c.f32()?,
                kwh_per_volume: c.f32()?,
                avg_kw: c.f32()?,
                peak_kw: c.f32()?,
                cost_per_day: c.f32()?,
            });
        }
        let demand_charge = c.f32()?;

        let mut nodes = Vec::with_capacity(n_periods * 4 * n_nodes);
        let mut links = Vec::with_capacity(n_periods * 8 * n_links);
        for _ in 0..n_periods {
            c.f32s(4 * n_nodes, &mut nodes)?;
            c.f32s(8 * n_links, &mut links)?;
        }

        Ok(SimResults {
            node_ids,
            link_ids,
            n_periods,
            report_start: ints[12].max(0) as u32,
            report_step: ints[13].max(0) as u32,
            duration: ints[14].max(0) as u32,
            flow_units: ints[9].max(0) as usize,
            pressure_units: ints[10].max(0) as usize,
            quality_flag: ints[7],
            chem_name,
            chem_units,
            pumps,
            demand_charge,
            warning,
            nodes,
            links,
            extra: Vec::new(),
        })
    }

    pub fn n_nodes(&self) -> usize {
        self.node_ids.len()
    }

    pub fn n_links(&self) -> usize {
        self.link_ids.len()
    }

    /// Values of a node variable for every node at `period`.
    pub fn node_slice(&self, period: usize, var: NodeVar) -> &[f32] {
        let n = self.n_nodes();
        let start = (period * 4 + var as usize) * n;
        &self.nodes[start..start + n]
    }

    /// Values of a link variable for every link at `period`.
    pub fn link_slice(&self, period: usize, var: LinkVar) -> &[f32] {
        let n = self.n_links();
        let start = (period * 8 + var as usize) * n;
        &self.links[start..start + n]
    }

    pub fn node_series(&self, node: usize, var: NodeVar) -> Vec<f32> {
        (0..self.n_periods)
            .map(|p| self.node_slice(p, var)[node])
            .collect()
    }

    pub fn link_series(&self, link: usize, var: LinkVar) -> Vec<f32> {
        (0..self.n_periods)
            .map(|p| self.link_slice(p, var)[link])
            .collect()
    }

    /// Simulation time (seconds) of a reporting period.
    pub fn period_time(&self, period: usize) -> u32 {
        self.report_start + period as u32 * self.report_step
    }

    pub fn is_si(&self) -> bool {
        self.flow_units >= 5
    }

    pub fn node_units(&self, var: NodeVar) -> String {
        match var {
            NodeVar::Demand => units::FLOW_UNITS
                .get(self.flow_units)
                .unwrap_or(&"")
                .to_string(),
            NodeVar::Head => units::length(self.is_si()).to_string(),
            NodeVar::Pressure => units::PRESSURE_UNITS
                .get(self.pressure_units)
                .unwrap_or(&"")
                .to_string(),
            NodeVar::Quality => self.quality_units(),
        }
    }

    pub fn link_units(&self, var: LinkVar) -> String {
        match var {
            LinkVar::Flow => units::FLOW_UNITS
                .get(self.flow_units)
                .unwrap_or(&"")
                .to_string(),
            LinkVar::Velocity => units::velocity(self.is_si()).to_string(),
            LinkVar::Headloss => units::headloss(self.is_si()).to_string(),
            LinkVar::Quality => self.quality_units(),
            LinkVar::Reaction => format!("{}/d", self.quality_units()),
            _ => String::new(),
        }
    }

    fn quality_units(&self) -> String {
        match self.quality_flag {
            2 => "hrs".into(),
            3 => "%".into(),
            _ => self.chem_units.clone(),
        }
    }
}

/// Formats seconds as `H:MM`.
pub fn format_time(secs: u32) -> String {
    format!("{}:{:02}", secs / 3600, (secs % 3600) / 60)
}
