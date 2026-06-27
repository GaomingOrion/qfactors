use std::collections::{HashMap, HashSet};

use polars::prelude::*;

use crate::error::{QFactorsError, Result};
use crate::factor::FactorResult;
use crate::group::GroupInfo;

pub const GROUP_ID_COL: &str = "__qfactors_group_id";
pub const TIME_ORD_COL: &str = "__qfactors_time_ord";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullPolicy {
    Error,
    FloatNullToNan,
}

impl NullPolicy {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "error" => Ok(Self::Error),
            "float_null_to_nan" => Ok(Self::FloatNullToNan),
            other => Err(QFactorsError::UnsupportedNullPolicy(other.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparePanelOptions {
    pub group_col: String,
    pub time_col: String,
    pub column_aliases: HashMap<String, String>,
    pub sort: bool,
    pub rechunk: bool,
    pub null_policy: NullPolicy,
    pub output_group_id: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedPanel {
    df: DataFrame,
    group_col: String,
    time_col: String,
    column_aliases: HashMap<String, String>,
    groups: Vec<GroupInfo>,
    unique_times: Column,
    output_group_id: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedObservation {
    pub input_index: usize,
    pub value: Column,
    pub ord_exclusive: usize,
}

impl PreparedPanel {
    pub fn new(mut df: DataFrame, options: PreparePanelOptions) -> Result<Self> {
        ensure_no_internal_column_conflicts(&df)?;
        ensure_column_exists(&df, &options.group_col)?;
        ensure_column_exists(&df, &options.time_col)?;
        apply_null_policy(&mut df, &options)?;

        if options.sort {
            df = sort_panel(&df, &options.group_col, &options.time_col)?;
        } else {
            ensure_panel_sorted(&df, &options.group_col, &options.time_col)?;
        }

        if options.rechunk {
            df.rechunk_mut();
        }

        let group_ids = build_groups(&df, &options.group_col, &options.time_col)?;
        let time_ordinals = build_time_ordinals(&df, &options.time_col)?;
        let groups = group_ids.groups;
        let unique_times = time_ordinals.unique_times;

        df.with_column(Column::new(GROUP_ID_COL.into(), group_ids.values))?;
        df.with_column(Column::new(TIME_ORD_COL.into(), time_ordinals.values))?;

        Ok(Self {
            df,
            group_col: options.group_col,
            time_col: options.time_col,
            column_aliases: options.column_aliases,
            groups,
            unique_times,
            output_group_id: options.output_group_id,
        })
    }

    pub fn dataframe(&self) -> &DataFrame {
        &self.df
    }

    pub fn group_col(&self) -> &str {
        &self.group_col
    }

    pub fn time_col(&self) -> &str {
        &self.time_col
    }

    pub fn column_aliases(&self) -> &HashMap<String, String> {
        &self.column_aliases
    }

    pub fn groups(&self) -> &[GroupInfo] {
        &self.groups
    }

    pub fn unique_times(&self) -> &Column {
        &self.unique_times
    }

    pub fn output_group_id(&self) -> bool {
        self.output_group_id
    }

    pub fn resolve_observation_times(
        &self,
        mut observation_times: Series,
    ) -> Result<Vec<PreparedObservation>> {
        observation_times.rename(self.time_col.clone().into());
        let mut observations = observation_times
            .cast(self.unique_times.dtype())?
            .into_column();
        observations.rename(self.time_col.clone().into());

        if observations.null_count() > 0 {
            return Err(QFactorsError::ObservationTimeNull);
        }

        ensure_unique_observation_times(&observations)?;

        let ord_exclusive_by_input = observation_ord_exclusive(&self.unique_times, &observations)?;
        Ok((0..observations.len())
            .map(|input_index| PreparedObservation {
                input_index,
                value: observations.new_from_index(input_index, 1),
                ord_exclusive: ord_exclusive_by_input[input_index],
            })
            .collect())
    }

    pub fn build_observation_frame(
        &self,
        observation: &PreparedObservation,
        factor_columns: FactorResult,
    ) -> Result<DataFrame> {
        let n_groups = self.groups.len();
        let mut time = observation.value.new_from_index(0, n_groups);
        time.rename(self.time_col.clone().into());

        let group_indices = self
            .groups
            .iter()
            .map(|group| group.start as IdxSize)
            .collect::<Vec<_>>();
        let mut group = self
            .df
            .column(&self.group_col)?
            .take_slice(&group_indices)?;
        group.rename(self.group_col.clone().into());

        let mut columns = vec![time, group];
        if self.output_group_id {
            let group_ids = self.groups.iter().map(|group| group.id).collect::<Vec<_>>();
            columns.push(Column::new(GROUP_ID_COL.into(), group_ids));
        }
        columns.extend(factor_columns);

        Ok(DataFrame::new_infer_height(columns)?)
    }
}

#[derive(Debug)]
struct GroupBuildResult {
    values: Vec<u32>,
    groups: Vec<GroupInfo>,
}

#[derive(Debug)]
struct TimeOrdinalBuildResult {
    values: Vec<u32>,
    unique_times: Column,
}

fn ensure_no_internal_column_conflicts(df: &DataFrame) -> Result<()> {
    for name in [GROUP_ID_COL, TIME_ORD_COL] {
        if df.get_column_index(name).is_some() {
            return Err(QFactorsError::InternalColumnConflict(name));
        }
    }
    Ok(())
}

fn ensure_column_exists(df: &DataFrame, name: &str) -> Result<()> {
    df.column(name)
        .map(|_| ())
        .map_err(|_| QFactorsError::MissingColumn(name.to_string()))
}

fn apply_null_policy(df: &mut DataFrame, options: &PreparePanelOptions) -> Result<()> {
    reject_nulls_in_required_column(df, &options.group_col, true)?;
    reject_nulls_in_required_column(df, &options.time_col, false)?;

    match options.null_policy {
        NullPolicy::Error => {
            for column in df.columns() {
                if column.null_count() > 0 {
                    return Err(QFactorsError::NullNotAllowed {
                        column: column.name().to_string(),
                    });
                }
            }
        }
        NullPolicy::FloatNullToNan => {
            let names = df.get_column_names_owned();
            for name in names {
                let index = df
                    .get_column_index(name.as_str())
                    .expect("name came from this DataFrame");
                let column = df.column(name.as_str())?;
                if column.null_count() == 0 {
                    continue;
                }

                if column.dtype() != &DataType::Float64 {
                    return Err(QFactorsError::FloatNullToNanTypeMismatch {
                        column: name.to_string(),
                        dtype: format!("{:?}", column.dtype()),
                    });
                }

                let values: Vec<f64> = column
                    .try_f64()
                    .expect("dtype checked above")
                    .iter()
                    .map(|value| value.unwrap_or(f64::NAN))
                    .collect();
                df.replace_column(index, Column::new(name, values))?;
            }
        }
    }

    Ok(())
}

fn reject_nulls_in_required_column(df: &DataFrame, name: &str, is_group: bool) -> Result<()> {
    let column = df.column(name)?;
    if column.null_count() == 0 {
        return Ok(());
    }

    if is_group {
        Err(QFactorsError::GroupNull(name.to_string()))
    } else {
        Err(QFactorsError::TimeNull(name.to_string()))
    }
}

fn sort_panel(df: &DataFrame, group_col: &str, time_col: &str) -> Result<DataFrame> {
    Ok(df.sort([group_col, time_col], SortMultipleOptions::default())?)
}

fn ensure_panel_sorted(df: &DataFrame, group_col: &str, time_col: &str) -> Result<()> {
    let sorted = sort_panel(df, group_col, time_col)?;
    if same_panel_order(df, &sorted, group_col, time_col)? {
        Ok(())
    } else {
        Err(QFactorsError::SortOrder {
            group_col: group_col.to_string(),
            time_col: time_col.to_string(),
        })
    }
}

fn same_panel_order(
    left: &DataFrame,
    right: &DataFrame,
    group_col: &str,
    time_col: &str,
) -> Result<bool> {
    if left.height() != right.height() {
        return Ok(false);
    }

    let left_group = left.column(group_col)?;
    let left_time = left.column(time_col)?;
    let right_group = right.column(group_col)?;
    let right_time = right.column(time_col)?;

    for row in 0..left.height() {
        if value_key(left_group, row)? != value_key(right_group, row)? {
            return Ok(false);
        }
        if value_key(left_time, row)? != value_key(right_time, row)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn build_groups(df: &DataFrame, group_col: &str, time_col: &str) -> Result<GroupBuildResult> {
    let group = df.column(group_col)?;
    let time = df.column(time_col)?;
    let mut values = Vec::with_capacity(df.height());
    let mut groups = Vec::new();

    let mut current_group: Option<String> = None;
    let mut previous_time: Option<String> = None;
    let mut current_start = 0usize;
    let mut current_id = 0u32;

    for row in 0..df.height() {
        let group_key = value_key(group, row)?;
        let time_key = value_key(time, row)?;

        match &current_group {
            None => {
                current_group = Some(group_key.clone());
                current_start = row;
            }
            Some(existing) if existing != &group_key => {
                groups.push(GroupInfo {
                    id: current_id,
                    label_key: existing.clone(),
                    start: current_start,
                    end: row,
                });
                current_id += 1;
                current_group = Some(group_key.clone());
                current_start = row;
            }
            Some(_) => {
                if previous_time.as_ref() == Some(&time_key) {
                    return Err(QFactorsError::DuplicateGroupTime {
                        group_col: group_col.to_string(),
                        time_col: time_col.to_string(),
                    });
                }
            }
        }

        values.push(current_id);
        previous_time = Some(time_key);
    }

    if let Some(label_key) = current_group {
        groups.push(GroupInfo {
            id: current_id,
            label_key,
            start: current_start,
            end: df.height(),
        });
    }

    Ok(GroupBuildResult { values, groups })
}

fn build_time_ordinals(df: &DataFrame, time_col: &str) -> Result<TimeOrdinalBuildResult> {
    let time = df.column(time_col)?;
    let mut time_only = DataFrame::new_infer_height(vec![time.clone()])?;
    time_only = time_only.sort([time_col], SortMultipleOptions::default())?;

    let sorted_time = time_only.column(time_col)?;
    let mut next_ord = 0u32;
    let mut previous_key: Option<String> = None;
    let mut ord_by_key = HashMap::new();
    let mut unique_indices = Vec::new();

    for row in 0..time_only.height() {
        let key = value_key(sorted_time, row)?;
        if previous_key.as_ref() != Some(&key) {
            ord_by_key.insert(key.clone(), next_ord);
            unique_indices.push(row as IdxSize);
            next_ord += 1;
            previous_key = Some(key);
        }
    }

    let mut ordinals = Vec::with_capacity(df.height());
    for row in 0..df.height() {
        let key = value_key(time, row)?;
        ordinals.push(
            *ord_by_key
                .get(&key)
                .expect("all row time values came from the unique time map"),
        );
    }

    let unique_times = sorted_time.take_slice(&unique_indices)?.rechunk();

    Ok(TimeOrdinalBuildResult {
        values: ordinals,
        unique_times,
    })
}

fn value_key(column: &Column, row: usize) -> Result<String> {
    let value = column.get(row)?;
    Ok(format!("{:?}:{:?}", column.dtype(), value))
}

fn ensure_unique_observation_times(observations: &Column) -> Result<()> {
    let mut seen = HashSet::with_capacity(observations.len());
    for row in 0..observations.len() {
        let key = value_key(observations, row)?;
        if !seen.insert(key.clone()) {
            return Err(QFactorsError::DuplicateObservationTime(key));
        }
    }
    Ok(())
}

fn observation_ord_exclusive(unique_times: &Column, observations: &Column) -> Result<Vec<usize>> {
    const KIND_COL: &str = "__qfactors_obs_kind";
    const INPUT_INDEX_COL: &str = "__qfactors_obs_input_index";

    let n_unique = unique_times.len();
    let n_obs = observations.len();
    let mut unique_time_col = unique_times.clone();
    unique_time_col.rename(observations.name().clone());

    let mut input_time_df = DataFrame::new_infer_height(vec![
        unique_time_col,
        Column::new(KIND_COL.into(), vec![0i32; n_unique]),
        Column::new(INPUT_INDEX_COL.into(), vec![-1i64; n_unique]),
    ])?;

    let mut obs_time_col = observations.clone();
    obs_time_col.rename(unique_times.name().clone());
    let obs_df = DataFrame::new_infer_height(vec![
        obs_time_col,
        Column::new(KIND_COL.into(), vec![1i32; n_obs]),
        Column::new(
            INPUT_INDEX_COL.into(),
            (0..n_obs as i64).collect::<Vec<_>>(),
        ),
    ])?;

    input_time_df.vstack_mut(&obs_df)?;
    let merged = input_time_df.sort(
        [observations.name().as_str(), KIND_COL],
        SortMultipleOptions::default(),
    )?;
    let kinds = merged
        .column(KIND_COL)?
        .try_i32()
        .expect("kind column is Int32");
    let input_indices = merged
        .column(INPUT_INDEX_COL)?
        .try_i64()
        .expect("input index column is Int64");

    let mut seen_unique = 0usize;
    let mut ord_exclusive_by_input = vec![0usize; n_obs];

    for row in 0..merged.height() {
        match kinds.get(row).expect("kind has no nulls") {
            0 => seen_unique += 1,
            1 => {
                let input_index =
                    input_indices.get(row).expect("input index has no nulls") as usize;
                ord_exclusive_by_input[input_index] = seen_unique;
            }
            _ => unreachable!("kind values are built locally"),
        }
    }

    Ok(ord_exclusive_by_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_options() -> PreparePanelOptions {
        PreparePanelOptions {
            group_col: "asset".to_string(),
            time_col: "time".to_string(),
            column_aliases: HashMap::new(),
            sort: true,
            rechunk: true,
            null_policy: NullPolicy::Error,
            output_group_id: false,
        }
    }

    #[test]
    fn prepare_panel_sorts_and_encodes_groups() -> Result<()> {
        let df = df!(
            "asset" => ["B", "A", "A"],
            "time" => [1i64, 2, 1],
            "close" => [20.0, 11.0, 10.0],
        )?;

        let panel = PreparedPanel::new(df, default_options())?;

        assert_eq!(panel.groups().len(), 2);
        assert_eq!(panel.groups()[0].start, 0);
        assert_eq!(panel.groups()[0].end, 2);
        assert_eq!(
            panel
                .dataframe()
                .column("asset")?
                .try_str()
                .expect("asset is string")
                .iter()
                .map(|value| value.expect("asset has no nulls"))
                .collect::<Vec<_>>(),
            ["A", "A", "B"]
        );
        assert!(panel.dataframe().column(GROUP_ID_COL).is_ok());
        assert!(panel.dataframe().column(TIME_ORD_COL).is_ok());

        Ok(())
    }

    #[test]
    fn sort_false_rejects_unsorted_input() {
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [2i64, 1],
            "close" => [11.0, 10.0],
        )
        .unwrap();
        let mut options = default_options();
        options.sort = false;

        let err = PreparedPanel::new(df, options).unwrap_err();
        assert!(matches!(err, QFactorsError::SortOrder { .. }));
    }

    #[test]
    fn duplicate_group_time_is_rejected() {
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [1i64, 1],
            "close" => [10.0, 11.0],
        )
        .unwrap();

        let err = PreparedPanel::new(df, default_options()).unwrap_err();
        assert!(matches!(err, QFactorsError::DuplicateGroupTime { .. }));
    }

    #[test]
    fn time_null_is_rejected() {
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [Some(1i64), None],
            "close" => [10.0, 11.0],
        )
        .unwrap();

        let err = PreparedPanel::new(df, default_options()).unwrap_err();
        assert!(matches!(err, QFactorsError::TimeNull(_)));
    }

    #[test]
    fn float_null_to_nan_replaces_float_nulls() -> Result<()> {
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [1i64, 2],
            "close" => [Some(10.0), None],
        )?;
        let mut options = default_options();
        options.null_policy = NullPolicy::FloatNullToNan;

        let panel = PreparedPanel::new(df, options)?;
        let values = panel
            .dataframe()
            .column("close")?
            .try_f64()
            .expect("close is f64")
            .into_no_null_iter()
            .collect::<Vec<_>>();
        assert!(values[1].is_nan());

        Ok(())
    }

    #[test]
    fn resolve_observation_times_keeps_input_order_and_uses_upper_bound() -> Result<()> {
        let df = df!(
            "asset" => ["A", "A", "A"],
            "time" => [1i64, 3, 5],
            "close" => [10.0, 11.0, 12.0],
        )?;
        let panel = PreparedPanel::new(df, default_options())?;

        let observations =
            panel.resolve_observation_times(Series::new("time".into(), [4i64, 0, 6]))?;

        assert_eq!(
            observations
                .iter()
                .map(|observation| observation.input_index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(
            observations
                .iter()
                .map(|observation| observation.ord_exclusive)
                .collect::<Vec<_>>(),
            [2, 0, 3]
        );

        Ok(())
    }

    #[test]
    fn resolve_observation_times_rejects_duplicates() {
        let df = df!(
            "asset" => ["A", "A"],
            "time" => [1i64, 2],
            "close" => [10.0, 11.0],
        )
        .unwrap();
        let panel = PreparedPanel::new(df, default_options()).unwrap();

        let err = panel
            .resolve_observation_times(Series::new("time".into(), [1i64, 1]))
            .unwrap_err();
        assert!(matches!(err, QFactorsError::DuplicateObservationTime(_)));
    }

    #[test]
    fn build_observation_frame_restores_group_labels() -> Result<()> {
        let df = df!(
            "asset" => ["B", "A", "A"],
            "time" => [1i64, 1, 2],
            "close" => [20.0, 10.0, 11.0],
        )?;
        let panel = PreparedPanel::new(df, default_options())?;
        let observations = panel.resolve_observation_times(Series::new("time".into(), [2i64]))?;
        let frame = panel.build_observation_frame(
            &observations[0],
            vec![Column::new("ret".into(), vec![0.1, f64::NAN])],
        )?;

        assert_eq!(
            frame
                .column("asset")?
                .try_str()
                .expect("asset is string")
                .iter()
                .map(|value| value.expect("asset has no nulls"))
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(frame.column("time")?.len(), 2);

        Ok(())
    }
}
