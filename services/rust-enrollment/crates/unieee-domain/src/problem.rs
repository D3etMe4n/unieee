use crate::RuleErrorCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const APPLICATION_PROBLEM_JSON: &str = "application/problem+json";
pub const PROBLEM_TYPE_PREFIX: &str = "urn:unieee:error:";

#[must_use]
pub const fn canonical_reason_phrase(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    }
}

/// Shared RFC 9457 Problem Details payload (`application/problem+json`) matching
/// the Warm Path (`core-java` `GlobalExceptionHandler` / `SecurityConfig`) wire format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetail {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub instance: String,
    pub error_code: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<BTreeMap<String, String>>,
}

impl ProblemDetail {
    #[must_use]
    pub fn new(
        status: u16,
        title: impl Into<String>,
        error_code: impl Into<String>,
        detail: impl Into<String>,
        instance: impl Into<String>,
    ) -> Self {
        let code = error_code.into();
        Self {
            problem_type: format!("{PROBLEM_TYPE_PREFIX}{code}"),
            title: title.into(),
            status,
            detail: detail.into(),
            instance: instance.into(),
            error_code: code,
            timestamp: Utc::now(),
            errors: None,
        }
    }

    #[must_use]
    pub fn for_status(
        status: u16,
        error_code: impl Into<String>,
        detail: impl Into<String>,
        instance: impl Into<String>,
    ) -> Self {
        Self::new(
            status,
            canonical_reason_phrase(status),
            error_code,
            detail,
            instance,
        )
    }

    #[must_use]
    pub fn validation(
        detail: impl Into<String>,
        instance: impl Into<String>,
        errors: BTreeMap<String, String>,
    ) -> Self {
        Self::new(
            400,
            "Validation Error",
            "VALIDATION_FAILED",
            detail,
            instance,
        )
        .with_errors(errors)
    }

    #[must_use]
    pub fn from_rule_error(
        code: RuleErrorCode,
        detail: impl Into<String>,
        instance: impl Into<String>,
    ) -> Self {
        Self::for_status(code.http_status_code(), code.as_str(), detail, instance)
    }

    #[must_use]
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = instance.into();
        self
    }

    #[must_use]
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    #[must_use]
    pub fn with_errors(mut self, errors: BTreeMap<String, String>) -> Self {
        self.errors = Some(errors);
        self
    }
}
