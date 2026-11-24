use unieee_domain::{EventId, SectionId, StudentId, TermId};

pub const KAFKA_TOPIC_ENROLLMENT_COMMANDS: &str = "unieee.enrollment.commands";
pub const KAFKA_TOPIC_ENROLLMENT_DLQ: &str = "unieee.enrollment.dlq";

pub const REDIS_CHANNEL_SEAT_TENTATIVE: &str = "unieee:seat:tentative";
pub const REDIS_CHANNEL_SEAT_CONFIRMED: &str = "unieee:seat:confirmed";
pub const REDIS_CHANNEL_ENROLLMENT_REVERT: &str = "unieee:enrollment:revert";
pub const REDIS_CHANNEL_GATE_CONTROL: &str = "unieee:gate:control";

#[must_use]
pub fn term_sections_cache_key(term_id: TermId) -> String {
    format!("unieee:cache:term:{term_id}:sections")
}

#[must_use]
pub fn section_seats_cache_key(section_id: SectionId) -> String {
    format!("unieee:cache:section:{section_id}:seats")
}

#[must_use]
pub fn student_timetable_cache_key(student_id: StudentId, term_id: TermId) -> String {
    format!("unieee:cache:student:{student_id}:term:{term_id}:timetable")
}

#[must_use]
pub fn policy_snapshot_cache_key(term_id: TermId) -> String {
    format!("unieee:cache:policy:{term_id}:snapshot")
}

#[must_use]
pub fn revert_dedup_key(event_id: EventId) -> String {
    format!("unieee:revert:dedup:{event_id}")
}
