use crate::core::{AuthError, EngineError, EngineReadiness};
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
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
        Self::with_title(status, title, error_code, detail)
    }

    #[must_use]
    pub fn with_title(
        status: StatusCode,
        title: impl Into<String>,
        error_code: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
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
    pub fn bad_request(error_code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, error_code, detail)
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

    #[must_use]
    pub fn with_errors(mut self, errors: BTreeMap<String, String>) -> Self {
        self.problem.errors = Some(errors);
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

/// JSON request extractor that normalizes all body/deserialization rejections to RFC 9457
/// `application/problem+json` with `instance` populated from the request URI path.
#[derive(Debug, Clone)]
pub struct ProblemJson<T>(pub T);

impl<S, T> FromRequest<S> for ProblemJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let instance = req.uri().path().to_owned();
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(payload)) => Ok(Self(payload)),
            Err(JsonRejection::MissingJsonContentType(_)) => Err(ApiError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "UNSUPPORTED_MEDIA_TYPE",
                "Request Content-Type must be application/json",
            )
            .with_instance(instance)),
            Err(JsonRejection::JsonDataError(err)) => Err(ApiError::validation(
                "Request validation failed",
                instance,
                BTreeMap::from([("body".to_owned(), err.body_text())]),
            )),
            Err(JsonRejection::JsonSyntaxError(err)) => {
                Err(ApiError::bad_request("INVALID_JSON", err.body_text()).with_instance(instance))
            }
            Err(JsonRejection::BytesRejection(err)) => Err(ApiError::bad_request(
                "INVALID_REQUEST_BODY",
                err.body_text(),
            )
            .with_instance(instance)),
            Err(other) => {
                Err(ApiError::bad_request("INVALID_REQUEST", other.body_text())
                    .with_instance(instance))
            }
        }
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
