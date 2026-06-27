use polars::prelude::*;

use crate::Result;

#[derive(Debug, Default)]
pub struct MemorySink {
    frames: Vec<DataFrame>,
}

impl MemorySink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_observation(&mut self, df: DataFrame) {
        self.frames.push(df);
    }

    pub fn finish(self) -> Result<DataFrame> {
        let mut frames = self.frames.into_iter();
        let Some(mut out) = frames.next() else {
            return Ok(DataFrame::default());
        };

        for frame in frames {
            out.vstack_mut(&frame)?;
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_sink_stacks_observations() -> Result<()> {
        let mut sink = MemorySink::new();
        sink.write_observation(df!("time" => [1i64], "ret" => [0.1])?);
        sink.write_observation(df!("time" => [2i64], "ret" => [0.2])?);

        let out = sink.finish()?;
        assert_eq!(out.height(), 2);
        assert_eq!(
            out.column("ret")?
                .try_f64()
                .expect("ret is f64")
                .into_no_null_iter()
                .collect::<Vec<_>>(),
            [0.1, 0.2]
        );
        Ok(())
    }
}
