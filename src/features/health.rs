// Health check feature
use crate::domain::HealthResponse;
use crate::error::Result;
use axum::Json;

#[derive(Clone)]
pub struct HealthHandlers;

impl HealthHandlers {
    pub fn new() -> Self {
        Self
    }

    /// GET /health - Health check
    pub async fn check(&self) -> Result<Json<HealthResponse>> {
        Ok(Json(HealthResponse::ok()))
    }
}

impl Default for HealthHandlers {
    fn default() -> Self {
        Self::new()
    }
}
