use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use serde::{Deserialize, Serialize};

use crate::server::endpoints;

#[derive(Deserialize)]
pub struct UpdateReadPositionRequest {
    pub url: String,
    pub scroll_position: f64,
}

#[derive(Serialize)]
pub struct UpdateReadPositionResponse {
    pub success: bool,
}

pub fn router() -> Router {
    Router::new().route(endpoints::UPDATE_READ_POSITION, post(update_read_position))
}

async fn update_read_position(Json(payload): Json<UpdateReadPositionRequest>) -> impl IntoResponse {
    println!(
        "Updating read position: url={}, scroll_position={}",
        payload.url, payload.scroll_position
    );

    (
        StatusCode::OK,
        Json(UpdateReadPositionResponse { success: true }),
    )
}
