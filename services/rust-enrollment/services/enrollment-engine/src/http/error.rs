use crate::core::{AuthError, EngineError, EngineReadiness};
use crate::http::dto::ErrorResponse;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub error_code: String,
    pub message: String,
}

impl ApiError {
    #[must_use]
    pub fn new(
        status: StatusCode,
        error_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            error_code: error_code.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn not_found(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, error_code, message)
    }

    #[must_use]
    pub fn bad_request(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, error_code, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            status: self.status.as_u16(),
            error_code: self.error_code,
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::Forbidden => Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", err.to_string()),
            AuthError::Expired => {
                Self::new(StatusCode::UNAUTHORIZED, "TOKEN_EXPIRED", err.to_string())
            }
            AuthError::JwksFetchFailed(_) => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "OIDC_JWKS_UNAVAILABLE",
                err.to_string(),
            ),
            other => Self::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", other.to_string()),
        }
    }
}

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> Self {
        match err {
            EngineError::NotReady(EngineReadiness::AwaitingPreload | EngineReadiness::Syncing) => {
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "ENGINE_NOT_READY",
                    err.to_string(),
                )
            }
            EngineError::NotReady(EngineReadiness::Ready) => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ENGINE_INTERNAL_ERROR",
                err.to_string(),
            ),
            EngineError::NodeStandby => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "NODE_STANDBY",
                err.to_string(),
            ),
            EngineError::RuleViolation { code, detail } => {
                let status = StatusCode::from_u16(code.http_status_code())
                    .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
                Self::new(status, code.as_str(), detail)
            }
            EngineError::Forbidden(msg) => Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", msg),
            EngineError::Domain(d) => {
                Self::new(StatusCode::BAD_REQUEST, "DOMAIN_ERROR", d.to_string())
            }
            EngineError::Internal(msg) => {
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
            }
        }
    }
}
