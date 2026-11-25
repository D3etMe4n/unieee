use chrono::Utc;
use rust_decimal::Decimal;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EventId, GateState, HotPathEnrollmentState,
    RuleErrorCode, SectionId, SectionStatus, StudentId, TermId, WindowId,
};
use unieee_events::{
    decode_json, decode_json_slice, encode_json, encode_json_bytes, event_status_cache_key,
    policy_snapshot_cache_key, revert_dedup_key, section_seats_cache_key,
    student_timetable_cache_key, term_sections_cache_key, DeadLetterEvent, EnrollmentCommandEvent,
    EnrollmentDeltaEvent, EventCodecError, EventStatusCachePayload, GateControlEvent,
    HotPathPubSubMessage, RevertEvent, SeatConfirmedEvent, SeatTentativeEvent,
    SectionCapacityUpdatedEvent, StudentStandingUpdatedEvent, EVENT_SCHEMA_VERSION_V1,
    KAFKA_TOPIC_ENROLLMENT_COMMANDS, KAFKA_TOPIC_ENROLLMENT_DELTAS, KAFKA_TOPIC_ENROLLMENT_DLQ,
    REDIS_CHANNEL_ENROLLMENT_REVERT, REDIS_CHANNEL_GATE_CONTROL, REDIS_CHANNEL_SEAT_CONFIRMED,
    REDIS_CHANNEL_SEAT_TENTATIVE,
};

#[test]
fn topic_and_redis_key_builders_match_database_and_cache_spec() {
    assert_eq!(
        KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        "unieee.enrollment.commands"
    );
    assert_eq!(KAFKA_TOPIC_ENROLLMENT_DLQ, "unieee.enrollment.dlq");
    assert_eq!(REDIS_CHANNEL_SEAT_TENTATIVE, "unieee:seat:tentative");
    assert_eq!(REDIS_CHANNEL_SEAT_CONFIRMED, "unieee:seat:confirmed");
    assert_eq!(REDIS_CHANNEL_ENROLLMENT_REVERT, "unieee:enrollment:revert");
    assert_eq!(REDIS_CHANNEL_GATE_CONTROL, "unieee:gate:control");

    let term_id = TermId::new_v4();
    let section_id = SectionId::new(1024);
    let student_id = StudentId::new_v4();
    let event_id = EventId::new_v7();

    assert_eq!(
        term_sections_cache_key(term_id),
        format!("unieee:cache:term:{term_id}:sections")
    );
    assert_eq!(
        section_seats_cache_key(section_id),
        "unieee:cache:section:1024:seats"
    );
    assert_eq!(
        student_timetable_cache_key(student_id, term_id),
        format!("unieee:cache:student:{student_id}:term:{term_id}:timetable")
    );
    assert_eq!(
        policy_snapshot_cache_key(term_id),
        format!("unieee:cache:policy:{term_id}:snapshot")
    );
    assert_eq!(
        revert_dedup_key(event_id),
        format!("unieee:revert:dedup:{event_id}")
    );
    assert_eq!(
        event_status_cache_key(event_id),
        format!("unieee:event:status:{event_id}")
    );
}

#[test]
fn kafka_command_and_dlq_events_roundtrip_with_camelcase_fields() {
    let now = Utc::now();
    let event = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        TermId::new_v4(),
        StudentId::new_v4(),
        SectionId::new(77),
        CourseId::new_v4(),
        EnrollmentAction::Register,
        4,
        now,
    );
    assert_eq!(event.partition_key(), "77");
    assert_eq!(event.seat_delta(), 1);

    let json = encode_json(&event).unwrap();
    assert!(json.contains("\"schemaVersion\":1"));
    assert!(json.contains("\"sectionId\":77"));
    assert!(json.contains("\"action\":\"REGISTER\""));
    assert!(json.contains("\"engineEpoch\":4"));

    let decoded: EnrollmentCommandEvent = decode_json(&json).unwrap();
    assert_eq!(decoded, event);

    let bytes = encode_json_bytes(&event).unwrap();
    let decoded_bytes: EnrollmentCommandEvent = decode_json_slice(&bytes).unwrap();
    assert_eq!(decoded_bytes, event);

    let dlq = DeadLetterEvent::new(
        Some(event.event_id),
        KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        3,
        912,
        "DESERIALIZATION_ERROR",
        "{bad}",
        now,
    );
    assert_eq!(dlq.partition_key(), event.event_id.to_string());
    let dlq_json = encode_json(&dlq).unwrap();
    assert_eq!(decode_json::<DeadLetterEvent>(&dlq_json).unwrap(), dlq);

    let dlq_no_id = DeadLetterEvent::new(
        None,
        KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        1,
        55,
        "ERR",
        "x",
        now,
    );
    assert_eq!(dlq_no_id.partition_key(), "unieee.enrollment.commands:1:55");
}

#[test]
fn pubsub_events_roundtrip_and_dispatch_by_channel() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let section_id = SectionId::new(501);
    let student_id = StudentId::new_v4();
    let course_id = CourseId::new_v4();
    let event_id = EventId::new_v7();

    let tentative = SeatTentativeEvent::new(term_id, section_id, 19, 50, 2, now);
    let confirmed = SeatConfirmedEvent::new(
        event_id,
        term_id,
        section_id,
        student_id,
        18,
        50,
        EnrollmentEventStatus::Confirmed,
        now,
    );
    let revert = RevertEvent::new(
        event_id,
        term_id,
        student_id,
        section_id,
        course_id,
        EnrollmentAction::Register,
        RuleErrorCode::DatabaseConstraintViolation,
        "CHECK constraint violated after admin capacity shrink",
        2,
        now,
    );
    let gate = GateControlEvent::new(term_id, GateState::Open, Some(WindowId::new_v4()), 2, now);

    let cases = vec![
        (
            HotPathPubSubMessage::SeatTentative(tentative.clone()),
            encode_json(&tentative).unwrap(),
        ),
        (
            HotPathPubSubMessage::SeatConfirmed(confirmed.clone()),
            encode_json(&confirmed).unwrap(),
        ),
        (
            HotPathPubSubMessage::Revert(revert.clone()),
            encode_json(&revert).unwrap(),
        ),
        (
            HotPathPubSubMessage::GateControl(gate.clone()),
            encode_json(&gate).unwrap(),
        ),
    ];

    for (msg, raw) in cases {
        let decoded = HotPathPubSubMessage::decode_channel_message(msg.channel(), &raw).unwrap();
        assert_eq!(decoded, msg);
    }

    assert!(matches!(
        HotPathPubSubMessage::decode_channel_message("unieee:unknown", "{}"),
        Err(EventCodecError::UnknownChannel(_))
    ));
}

#[test]
fn codec_rejects_unsupported_schema_version_and_malformed_json() {
    let mut bad_event =
        GateControlEvent::new(TermId::new_v4(), GateState::Paused, None, 1, Utc::now());
    bad_event.schema_version = EVENT_SCHEMA_VERSION_V1 + 1;

    assert!(matches!(
        encode_json(&bad_event),
        Err(EventCodecError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 2
        })
    ));
    assert!(matches!(
        encode_json_bytes(&bad_event),
        Err(EventCodecError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 2
        })
    ));

    let bad_json = serde_json::to_string(&bad_event).unwrap();
    assert!(matches!(
        decode_json::<GateControlEvent>(&bad_json),
        Err(EventCodecError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 2
        })
    ));
    assert!(matches!(
        decode_json_slice::<GateControlEvent>(bad_json.as_bytes()),
        Err(EventCodecError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 2
        })
    ));
    assert!(matches!(
        decode_json::<GateControlEvent>("{not-json"),
        Err(EventCodecError::Deserialize(_))
    ));
}

#[test]
fn hot_delta_sync_events_roundtrip_and_validate_schema_version() {
    assert_eq!(KAFKA_TOPIC_ENROLLMENT_DELTAS, "unieee.enrollment.deltas");

    let now = Utc::now();
    let term_id = TermId::new_v4();
    let student_id = StudentId::new_v4();
    let standing_event_id = EventId::new_v7();
    let standing = StudentStandingUpdatedEvent::new(
        standing_event_id,
        term_id,
        student_id,
        "ACTIVE",
        Decimal::new(0, 2),
        false,
        false,
        Some(Decimal::new(355, 2)),
        now,
    );
    assert_eq!(standing.partition_key(), student_id.to_string());
    let delta_standing = EnrollmentDeltaEvent::StudentStandingUpdated(standing.clone());
    assert_eq!(delta_standing.event_id(), Some(standing_event_id));
    assert_eq!(delta_standing.term_id(), term_id);
    assert_eq!(delta_standing.occurred_at(), now);
    assert_eq!(delta_standing.partition_key(), student_id.to_string());

    let standing_json = encode_json(&delta_standing).unwrap();
    assert!(standing_json.contains("\"deltaType\":\"STUDENT_STANDING_UPDATED\""));
    assert!(standing_json.contains("\"tuitionDebt\":\"0.00\""));
    assert_eq!(
        decode_json::<EnrollmentDeltaEvent>(&standing_json).unwrap(),
        delta_standing
    );

    let cap_event_id = EventId::new_v7();
    let cap = SectionCapacityUpdatedEvent::new(
        cap_event_id,
        term_id,
        SectionId::new(888),
        120,
        Some(SectionStatus::Open),
        now,
    );
    assert_eq!(cap.partition_key(), "888");
    let delta_cap = EnrollmentDeltaEvent::SectionCapacityUpdated(cap.clone());
    assert_eq!(delta_cap.event_id(), Some(cap_event_id));
    assert_eq!(delta_cap.term_id(), term_id);
    assert_eq!(delta_cap.occurred_at(), now);
    assert_eq!(delta_cap.partition_key(), "888");

    let cap_bytes = encode_json_bytes(&delta_cap).unwrap();
    assert_eq!(
        decode_json_slice::<EnrollmentDeltaEvent>(&cap_bytes).unwrap(),
        delta_cap
    );

    let revert_id = EventId::new_v7();
    let course_id = CourseId::new_v4();
    let revert = RevertEvent::new(
        revert_id,
        term_id,
        student_id,
        SectionId::new(888),
        course_id,
        EnrollmentAction::Register,
        RuleErrorCode::DatabaseConstraintViolation,
        "PG CHECK failed",
        3,
        now,
    );
    let delta_revert = EnrollmentDeltaEvent::Revert(revert);
    assert_eq!(delta_revert.event_id(), Some(revert_id));
    assert_eq!(delta_revert.term_id(), term_id);
    assert_eq!(delta_revert.partition_key(), student_id.to_string());
    assert_eq!(delta_revert.occurred_at(), now);
    let revert_json = encode_json(&delta_revert).unwrap();
    assert!(revert_json.contains("\"deltaType\":\"REVERT\""));
    assert_eq!(
        decode_json::<EnrollmentDeltaEvent>(&revert_json).unwrap(),
        delta_revert
    );

    let gate = GateControlEvent::new(term_id, GateState::Open, Some(WindowId::new_v4()), 4, now);
    let delta_gate = EnrollmentDeltaEvent::GateControl(gate);
    assert_eq!(delta_gate.event_id(), None);
    assert_eq!(delta_gate.term_id(), term_id);
    assert_eq!(delta_gate.partition_key(), term_id.to_string());
    assert_eq!(delta_gate.occurred_at(), now);
    let gate_json = encode_json(&delta_gate).unwrap();
    assert!(gate_json.contains("\"deltaType\":\"GATE_CONTROL\""));
    assert_eq!(
        decode_json::<EnrollmentDeltaEvent>(&gate_json).unwrap(),
        delta_gate
    );

    let cached_status = EventStatusCachePayload::new(
        revert_id,
        term_id,
        student_id,
        SectionId::new(888),
        course_id,
        EnrollmentAction::Register,
        HotPathEnrollmentState::Reverted,
        Some(RuleErrorCode::DatabaseConstraintViolation),
        3,
        now,
    );
    let status_json = encode_json(&cached_status).unwrap();
    assert_eq!(
        decode_json::<EventStatusCachePayload>(&status_json).unwrap(),
        cached_status
    );

    let mut bad_standing = standing;
    bad_standing.schema_version = EVENT_SCHEMA_VERSION_V1 + 1;
    let bad_delta = EnrollmentDeltaEvent::StudentStandingUpdated(bad_standing);
    assert!(matches!(
        encode_json(&bad_delta),
        Err(EventCodecError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 2
        })
    ));

    let mut bad_cap = cap;
    bad_cap.schema_version = EVENT_SCHEMA_VERSION_V1 + 1;
    let bad_cap_delta = EnrollmentDeltaEvent::SectionCapacityUpdated(bad_cap);
    assert!(matches!(
        encode_json(&bad_cap_delta),
        Err(EventCodecError::UnsupportedSchemaVersion {
            expected: 1,
            actual: 2
        })
    ));
}
