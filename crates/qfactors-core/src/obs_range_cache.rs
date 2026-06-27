use std::collections::HashMap;
use std::ops::Range;

use crate::error::{QFactorsError, Result};
use crate::prepared_panel::{PreparedPanel, TIME_ORD_COL};

#[derive(Debug, Clone)]
pub struct ObsRangeCache {
    pub obs_ord_exclusive: usize,
    pub ranges_by_window: HashMap<usize, Vec<Option<Range<usize>>>>,
}

impl ObsRangeCache {
    pub fn new(panel: &PreparedPanel, obs_ord_exclusive: usize, windows: &[usize]) -> Result<Self> {
        let time_ordinals = panel
            .dataframe()
            .column(TIME_ORD_COL)?
            .try_u32()
            .expect("prepared panel stores time ordinals as UInt32")
            .cont_slice()
            .map_err(|_| QFactorsError::NonContiguousColumn(TIME_ORD_COL.to_string()))?;

        let mut ranges_by_window = HashMap::with_capacity(windows.len());
        for &window in windows {
            let mut ranges = Vec::with_capacity(panel.groups().len());
            for group in panel.groups() {
                let group_time_ordinals = &time_ordinals[group.start..group.end];
                let end = group_time_ordinals
                    .partition_point(|ordinal| (*ordinal as usize) < obs_ord_exclusive);
                let start = end.saturating_sub(window);

                if end - start < window {
                    ranges.push(None);
                } else {
                    ranges.push(Some(group.start + start..group.start + end));
                }
            }
            ranges_by_window.insert(window, ranges);
        }

        Ok(Self {
            obs_ord_exclusive,
            ranges_by_window,
        })
    }

    pub fn ranges_for(&self, window: usize) -> Result<&[Option<Range<usize>>]> {
        self.ranges_by_window
            .get(&window)
            .map(Vec::as_slice)
            .ok_or(QFactorsError::MissingWindowRange(window))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use polars::prelude::*;

    use super::*;
    use crate::{NullPolicy, PreparePanelOptions, PreparedPanel};

    fn panel() -> Result<PreparedPanel> {
        let df = df!(
            "asset" => ["A", "A", "A", "B", "B"],
            "time" => [1i64, 2, 3, 1, 3],
            "close" => [10.0, 11.0, 12.0, 20.0, 22.0],
        )?;
        PreparedPanel::new(
            df,
            PreparePanelOptions {
                group_col: "asset".to_string(),
                time_col: "time".to_string(),
                column_aliases: HashMap::new(),
                sort: true,
                rechunk: true,
                null_policy: NullPolicy::Error,
                output_group_id: false,
            },
        )
    }

    #[test]
    fn builds_group_local_ranges() -> Result<()> {
        let panel = panel()?;
        let cache = ObsRangeCache::new(&panel, 3, &[2])?;
        let ranges = cache.ranges_for(2)?;

        assert_eq!(ranges[0], Some(1..3));
        assert_eq!(ranges[1], Some(3..5));
        Ok(())
    }

    #[test]
    fn insufficient_window_is_none() -> Result<()> {
        let panel = panel()?;
        let cache = ObsRangeCache::new(&panel, 1, &[2])?;
        let ranges = cache.ranges_for(2)?;

        assert_eq!(ranges, &[None, None]);
        Ok(())
    }
}
