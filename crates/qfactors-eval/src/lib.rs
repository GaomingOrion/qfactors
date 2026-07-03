pub mod context;
pub mod error;
pub mod evaluate;
pub mod labels;
pub mod metrics;
pub(crate) mod panel;
pub mod stats;

pub use context::{Binning, Demean, EvalContext, EvalSpec};
pub use error::{EvalError, Result};
pub use evaluate::{EvalOutput, EvaluateOptions, TableData, evaluate, save_output};
pub use labels::{LabelOptions, LabelsOutput, with_labels};
