use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

pub async fn get_models(State(state): State<Arc<AppState>>) -> Response {
	let models = state.models.read().await;
	match models.as_ref() {
		Some(m) => Json(m.clone()).into_response(),
		None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
	}
}
