use std::collections::HashMap;
use std::sync::OnceLock;

use linkme::distributed_slice;

use crate::error::{QFactorsError, Result};
use crate::factor::FactorDescriptor;

#[distributed_slice]
pub static FACTOR_DESCRIPTORS: [fn() -> FactorDescriptor];

#[derive(Debug)]
pub struct FactorRegistry {
    factors: HashMap<&'static str, FactorDescriptor>,
}

impl FactorRegistry {
    pub fn from_descriptors(descriptors: Vec<FactorDescriptor>) -> Result<Self> {
        let mut factors = HashMap::with_capacity(descriptors.len());
        for descriptor in descriptors {
            let factor_name = descriptor.factor_name;
            if factors.insert(factor_name, descriptor).is_some() {
                return Err(QFactorsError::DuplicateFactorName(factor_name.to_string()));
            }
        }
        Ok(Self { factors })
    }

    pub fn get(&self, factor_name: &str) -> Option<&FactorDescriptor> {
        self.factors.get(factor_name)
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &FactorDescriptor> {
        self.factors.values()
    }
}

static REGISTRY: OnceLock<FactorRegistry> = OnceLock::new();

pub fn factor_registry() -> Result<&'static FactorRegistry> {
    if let Some(registry) = REGISTRY.get() {
        return Ok(registry);
    }

    let descriptors = FACTOR_DESCRIPTORS
        .iter()
        .map(|factory| factory())
        .collect::<Vec<_>>();
    let registry = FactorRegistry::from_descriptors(descriptors)?;
    let _ = REGISTRY.set(registry);
    Ok(REGISTRY.get().expect("registry was set above"))
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use polars::prelude::Column;

    use super::*;
    use crate::column_store::ColumnStore;
    use crate::factor::{ColumnSpec, DType, FactorResult, ResolvedFactor};

    static INPUTS: [ColumnSpec; 1] = [ColumnSpec {
        name: "close",
        dtype: DType::F64,
    }];
    static OUTPUTS: [ColumnSpec; 1] = [ColumnSpec {
        name: "dummy",
        dtype: DType::F64,
    }];

    fn dummy_descriptor() -> FactorDescriptor {
        FactorDescriptor {
            factor_name: "dummy",
            kernel_name: "dummy",
            window: 2,
            inputs: &INPUTS,
            outputs: &OUTPUTS,
            param_set: None,
            params: &[],
            compute: dummy_compute,
        }
    }

    fn dummy_compute(
        _columns: &ColumnStore<'_>,
        ranges: &[Option<Range<usize>>],
        factor: &ResolvedFactor<'_>,
    ) -> Result<FactorResult> {
        Ok(vec![Column::new(
            factor.output_columns[0].clone().into(),
            vec![f64::NAN; ranges.len()],
        )])
    }

    #[test]
    fn registry_rejects_duplicate_factor_names() {
        let err = FactorRegistry::from_descriptors(vec![dummy_descriptor(), dummy_descriptor()])
            .unwrap_err();

        assert!(matches!(err, QFactorsError::DuplicateFactorName(name) if name == "dummy"));
    }
}
