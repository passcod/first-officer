use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::info;

use crate::state::AppState;

pub async fn get_models(State(state): State<Arc<AppState>>) -> Response {
	let models = state.models.read().await;
	match models.as_ref() {
		Some(m) => {
			info!(count = m.data.len(), "serving models list");
			Json(m.clone()).into_response()
		}
		None => {
			info!("models not yet cached, returning 503");
			StatusCode::SERVICE_UNAVAILABLE.into_response()
		}
	}
}
