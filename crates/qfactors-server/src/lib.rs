//! Read-only HTTP server over a saved evaluation `output_dir`. Serves the table
//! JSON the Vue frontend consumes and, when built, the frontend itself.

mod data;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

pub use data::{DataDir, DataError};

#[derive(Clone)]
struct AppState {
    dir: Arc<DataDir>,
}

/// Build the API router. When `assets` points at a built frontend (`dist`), it
/// is served at `/` with SPA fallback; otherwise only `/api/*` is available.
pub fn router(dir: DataDir, assets: Option<PathBuf>) -> Router {
    let state = AppState { dir: Arc::new(dir) };
    let api = Router::new()
        .route("/api/meta", get(meta))
        .route("/api/summary", get(summary))
        .route("/api/factor/{name}", get(factor))
        .with_state(state)
        .layer(CorsLayer::permissive());

    match assets.filter(|p| p.join("index.html").is_file()) {
        Some(dist) => {
            let index = dist.join("index.html");
            let files = ServeDir::new(&dist).not_found_service(ServeFile::new(index));
            api.fallback_service(files)
        }
        None => api.fallback(get(no_assets)),
    }
}

async fn no_assets() -> impl IntoResponse {
    (
        StatusCode::OK,
        "qfactors-server: API is up at /api/*. Build the frontend \
         (cd frontend && npm run build) or pass --assets to serve the UI.",
    )
}

async fn meta(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let raw = s.dir.meta_json()?;
    let mut value: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    if let Value::Object(map) = &mut value {
        map.insert("factors".into(), json!(s.dir.factors()?));
    }
    Ok(Json(value))
}

async fn summary(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(s.dir.summary_records()?))
}

async fn factor(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(s.dir.factor_bundle(&name)?))
}

/// API error → HTTP status. A missing table/factor is a 404; anything else is a
/// 500 with the polars/IO message (this is a localhost dev tool).
struct ApiError(StatusCode, String);

impl From<DataError> for ApiError {
    fn from(err: DataError) -> Self {
        let status = match err {
            DataError::Missing(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(status, err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use qfactors_core::PanelOptions;
    use qfactors_eval::{Binning, Demean, EvaluateOptions, Weighting, evaluate};
    use polars::prelude::*;
    use tower::ServiceExt;

    fn fixture_dir() -> PathBuf {
        let df = df!(
            "asset" => ["A", "B", "C", "D", "A", "B", "C", "D"],
            "time" => [20481i32, 20481, 20481, 20481, 20512, 20512, 20512, 20512],
            "f1" => [1.0f64, 2.0, 3.0, 4.0, 4.0, 3.0, 2.0, 1.0],
            "ret_1" => [0.01f64, 0.02, 0.03, 0.04, 0.04, 0.03, 0.02, 0.01],
        )
        .unwrap();
        let mut df = df;
        let time = df.column("time").unwrap().cast(&DataType::Date).unwrap();
        df.with_column(time).unwrap();

        let panel = PanelOptions {
            symbol_col: "asset".into(),
            time_col: "time".into(),
        };
        let dir = std::env::temp_dir().join(format!("qf-server-test-{}", std::process::id()));
        let opts = EvaluateOptions {
            factor_cols: vec!["f1".into()],
            label_cols: None,
            quantiles: 2,
            binning: Binning::Daily,
            demean: Demean::None,
            min_cs_count: 2,
            group_col: None,
            tradable_col: None,
            cost_bps: 0.0,
            weighting: Weighting::Factor,
            factor_source: None,
            output_dir: Some(dir.to_string_lossy().into_owned()),
        };
        evaluate(&df, &panel, &opts).unwrap();
        dir
    }

    async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
        let res = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn serves_meta_summary_and_factor() {
        let dir = fixture_dir();
        let app = router(DataDir::new(&dir).unwrap(), None);

        let (status, meta) = get_json(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(meta["factors"], json!(["f1"]));
        assert_eq!(meta["quantiles"], json!(2));

        let (status, summary) = get_json(&app, "/api/summary").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(summary.as_array().unwrap()[0]["factor"], json!("f1"));

        let (status, bundle) = get_json(&app, "/api/factor/f1").await;
        assert_eq!(status, StatusCode::OK);
        assert!(bundle["ic"]["ic"].as_array().unwrap().len() >= 2);
        // Date column reached JSON as an ISO string, not an integer.
        assert!(bundle["ic"]["date"][0].as_str().unwrap().contains('-'));
        assert!(bundle["quantiles"]["mean_ret_1"].is_array());
        assert!(bundle["monthly"].is_object());

        let (status, _) = get_json(&app, "/api/factor/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(&dir).ok();
    }
}
