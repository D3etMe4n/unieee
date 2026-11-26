use crate::core::{AuthError, SubscribeError};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::BTreeMap;
use unieee_domain::{canonical_reason_phrase, ProblemDetail, APPLICATION_PROBLEM_JSON};

const FALLBACK_PROBLEM_JSON: &[u8] = br#"{"type":"urn:unieee:error:INTERNAL_ERROR","title":"Internal Server Error","status":500,"detail":"Failed to serialize RFC 9457 problem response","instance":"","errorCode":"INTERNAL_ERROR","timestamp":"1970-01-01T00:00:00Z"}"#;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub problem: Box<ProblemDetail>,
}

impl ApiError {
    #[must_use]
    pub fn new(
        status: StatusCode,
        error_code: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        let title = status
            .canonical_reason()
            .unwrap_or_else(|| canonical_reason_phrase(status.as_u16()));
        Self {
            status,
            problem: Box::new(ProblemDetail::new(
                status.as_u16(),
                title,
                error_code,
                detail,
                "",
            )),
        }
    }

    #[must_use]
    pub fn not_found(error_code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, error_code, detail)
    }

    #[must_use]
    pub fn validation(
        detail: impl Into<String>,
        instance: impl Into<String>,
        errors: BTreeMap<String, String>,
    ) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            problem: Box::new(ProblemDetail::validation(detail, instance, errors)),
        }
    }

    #[must_use]
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.problem.instance = instance.into();
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body =
            serde_json::to_vec(&self.problem).unwrap_or_else(|_| FALLBACK_PROBLEM_JSON.to_vec());
        (
            self.status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(APPLICATION_PROBLEM_JSON),
            )],
            body,
        )
            .into_response()
    }
}

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::Forbidden => Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", err.to_string()),
            AuthError::MissingStudentIdClaim => Self::new(
                StatusCode::FORBIDDEN,
                "MISSING_STUDENT_ID_CLAIM",
                err.to_string(),
            ),
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

impl From<SubscribeError> for ApiError {
    fn from(err: SubscribeError) -> Self {
        match err {
            SubscribeError::NotReady(_) => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "SSE_GATEWAY_NOT_READY",
                err.to_string(),
            ),
            SubscribeError::MaxConnectionsReached { .. } => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "MAX_CONNECTIONS_REACHED",
                err.to_string(),
            ),
        }
    }
}
