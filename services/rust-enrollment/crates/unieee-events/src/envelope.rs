use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

pub const EVENT_SCHEMA_VERSION_V1: u16 = 1;

pub trait VersionedEvent {
    const SCHEMA_VERSION: u16 = EVENT_SCHEMA_VERSION_V1;
    fn schema_version(&self) -> u16;
}

#[derive(Debug, Error)]
pub enum EventCodecError {
    #[error("failed to serialize event payload: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to deserialize event payload: {0}")]
    Deserialize(serde_json::Error),
    #[error("unsupported schemaVersion: expected {expected}, got {actual}")]
    UnsupportedSchemaVersion { expected: u16, actual: u16 },
    #[error("unknown Redis Pub/Sub channel: {0}")]
    UnknownChannel(String),
}

pub fn encode_json<T>(event: &T) -> Result<String, EventCodecError>
where
    T: Serialize + VersionedEvent,
{
    validate_version::<T>(event.schema_version())?;
    Ok(serde_json::to_string(event)?)
}

pub fn encode_json_bytes<T>(event: &T) -> Result<Vec<u8>, EventCodecError>
where
    T: Serialize + VersionedEvent,
{
    validate_version::<T>(event.schema_version())?;
    Ok(serde_json::to_vec(event)?)
}

pub fn decode_json<T>(raw: &str) -> Result<T, EventCodecError>
where
    T: DeserializeOwned + VersionedEvent,
{
    let event: T = serde_json::from_str(raw).map_err(EventCodecError::Deserialize)?;
    validate_version::<T>(event.schema_version())?;
    Ok(event)
}

pub fn decode_json_slice<T>(raw: &[u8]) -> Result<T, EventCodecError>
where
    T: DeserializeOwned + VersionedEvent,
{
    let event: T = serde_json::from_slice(raw).map_err(EventCodecError::Deserialize)?;
    validate_version::<T>(event.schema_version())?;
    Ok(event)
}

fn validate_version<T: VersionedEvent>(actual: u16) -> Result<(), EventCodecError> {
    if actual != T::SCHEMA_VERSION {
        return Err(EventCodecError::UnsupportedSchemaVersion {
            expected: T::SCHEMA_VERSION,
            actual,
        });
    }
    Ok(())
}
