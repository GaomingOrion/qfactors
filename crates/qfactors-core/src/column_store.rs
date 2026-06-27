use polars::prelude::*;

use crate::error::{QFactorsError, Result};
use crate::factor::DType;

#[derive(Debug, Clone, Copy)]
pub struct ColumnStore<'a> {
    df: &'a DataFrame,
}

impl<'a> ColumnStore<'a> {
    pub fn new(df: &'a DataFrame) -> Self {
        Self { df }
    }

    pub fn f64(&self, name: &str) -> Result<&'a [f64]> {
        let column = self.column(name)?;
        ensure_dtype(column, DType::F64)?;
        ensure_no_nulls(column)?;
        column
            .try_f64()
            .expect("dtype checked above")
            .cont_slice()
            .map_err(|_| QFactorsError::NonContiguousColumn(name.to_string()))
    }

    pub fn u32(&self, name: &str) -> Result<&'a [u32]> {
        let column = self.column(name)?;
        ensure_dtype(column, DType::U32)?;
        ensure_no_nulls(column)?;
        column
            .try_u32()
            .expect("dtype checked above")
            .cont_slice()
            .map_err(|_| QFactorsError::NonContiguousColumn(name.to_string()))
    }

    pub fn i64(&self, name: &str) -> Result<&'a [i64]> {
        let column = self.column(name)?;
        ensure_dtype(column, DType::I64)?;
        ensure_no_nulls(column)?;
        column
            .try_i64()
            .expect("dtype checked above")
            .cont_slice()
            .map_err(|_| QFactorsError::NonContiguousColumn(name.to_string()))
    }

    fn column(&self, name: &str) -> Result<&'a Column> {
        self.df
            .column(name)
            .map_err(|_| QFactorsError::MissingColumn(name.to_string()))
    }
}

pub fn ensure_dtype(column: &Column, expected: DType) -> Result<()> {
    let matches = match expected {
        DType::F64 => column.dtype() == &DataType::Float64,
        DType::U32 => column.dtype() == &DataType::UInt32,
        DType::I64 => column.dtype() == &DataType::Int64,
    };

    if matches {
        Ok(())
    } else {
        Err(QFactorsError::DTypeMismatch {
            column: column.name().to_string(),
            expected: expected.name(),
            actual: format!("{:?}", column.dtype()),
        })
    }
}

pub fn ensure_no_nulls(column: &Column) -> Result<()> {
    if column.null_count() == 0 {
        Ok(())
    } else {
        Err(QFactorsError::NullNotAllowed {
            column: column.name().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f64_returns_contiguous_slice() -> Result<()> {
        let df = df!("close" => [1.0, 2.0, 3.0])?;
        let store = ColumnStore::new(&df);

        assert_eq!(store.f64("close")?, &[1.0, 2.0, 3.0]);
        Ok(())
    }

    #[test]
    fn f64_rejects_wrong_dtype() {
        let df = df!("close" => [1u32, 2, 3]).unwrap();
        let store = ColumnStore::new(&df);

        let err = store.f64("close").unwrap_err();
        assert!(matches!(err, QFactorsError::DTypeMismatch { .. }));
    }

    #[test]
    fn f64_rejects_nulls() {
        let df = df!("close" => [Some(1.0), None]).unwrap();
        let store = ColumnStore::new(&df);

        let err = store.f64("close").unwrap_err();
        assert!(matches!(err, QFactorsError::NullNotAllowed { .. }));
    }
}
