use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::{Uuid, Version};

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }

            #[must_use]
            pub fn new_v4() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    };
}

uuid_id!(TermId);
uuid_id!(CourseId);
uuid_id!(StudentId);
uuid_id!(PolicyId);
uuid_id!(WindowId);
uuid_id!(EventId);

impl EventId {
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub fn is_v7(&self) -> bool {
        self.0.get_version() == Some(Version::SortRand)
    }

    /// Extract UTC timestamp from UUIDv7 `EventId`; returns `None` for non-v7 UUIDs.
    #[must_use]
    pub fn v7_timestamp_utc(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        if !self.is_v7() {
            return None;
        }
        let ts = self.0.get_timestamp()?;
        let (secs, nanos) = ts.to_unix();
        chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, nanos)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SectionId(u64);

impl SectionId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn try_from_pg(id: i64) -> Result<Self, DomainError> {
        if id <= 0 {
            return Err(DomainError::InvalidIdentifier {
                kind: "SectionId",
                reason: format!("PostgreSQL BIGINT section id must be positive, got {id}"),
            });
        }
        Ok(Self(id as u64))
    }

    pub fn to_pg_i64(&self) -> Result<i64, DomainError> {
        if self.0 == 0 || self.0 > i64::MAX as u64 {
            return Err(DomainError::InvalidIdentifier {
                kind: "SectionId",
                reason: format!(
                    "SectionId {} cannot be represented as positive PostgreSQL BIGINT",
                    self.0
                ),
            });
        }
        Ok(self.0 as i64)
    }
}

impl From<u64> for SectionId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<SectionId> for u64 {
    fn from(value: SectionId) -> Self {
        value.0
    }
}

impl fmt::Display for SectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SectionId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>().map(Self)
    }
}
