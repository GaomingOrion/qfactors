use std::ops::Range;

use polars::prelude::Column;

use crate::column_store::ColumnStore;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F64,
    U32,
    I64,
}

impl DType {
    pub fn name(self) -> &'static str {
        match self {
            Self::F64 => "Float64",
            Self::U32 => "UInt32",
            Self::I64 => "Int64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    pub name: &'static str,
    pub dtype: DType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub value: ParamValue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    F64(f64),
}

pub type FactorResult = Vec<Column>;

pub type FactorComputeFn = fn(
    columns: &ColumnStore<'_>,
    ranges: &[Option<Range<usize>>],
    factor: &ResolvedFactor<'_>,
) -> Result<FactorResult>;

#[derive(Debug, Clone)]
pub struct FactorDescriptor {
    pub factor_name: &'static str,
    pub kernel_name: &'static str,
    pub window: usize,
    pub inputs: &'static [ColumnSpec],
    pub outputs: &'static [ColumnSpec],
    pub param_set: Option<&'static str>,
    pub params: &'static [ParamSpec],
    pub compute: FactorComputeFn,
}

#[derive(Debug, Clone)]
pub struct ResolvedFactor<'a> {
    pub desc: &'a FactorDescriptor,
    pub input_columns: Vec<String>,
    pub output_columns: Vec<String>,
}

pub fn default_output_columns(desc: &FactorDescriptor) -> Vec<String> {
    if desc.outputs.len() == 1 {
        vec![desc.factor_name.to_string()]
    } else {
        desc.outputs
            .iter()
            .map(|output| format!("{}.{}", desc.factor_name, output.name))
            .collect()
    }
}
