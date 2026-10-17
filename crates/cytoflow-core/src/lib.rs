//! CytoFlow core: a clean-room flow cytometry analysis engine.
//!
//! Everything here is implemented from public specifications and papers:
//! the ISAC FCS 3.1 standard, Gating-ML 2.0, Parks/Roederer/Moore 2006
//! (logicle), and the publicly documented FlowJo `.wsp` XML layout.

pub mod compensation;
pub mod error;
pub mod export;
pub mod fcs;
pub mod gate;
pub mod gatingml;
pub mod plot;
pub mod stats;
pub mod transform;
pub mod workspace;
pub mod wsp;
pub mod xml;

pub use error::{Error, Result};
pub use fcs::{FcsFile, Parameter};
pub use gate::{Gate, GateDim, GateNode, GateShape};
pub use transform::{Transform, TransformKind};
pub use workspace::{ChannelSetting, Workspace};
