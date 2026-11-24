use chrono::Utc;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EventId, GateState, RuleErrorCode,
    SectionId, StudentId, TermId, WindowId,
};
use unieee_events::{
    decode_json, decode_json_slice, encode_json, encode_json_bytes, policy_snapshot_cache_key,
    revert_dedup_key, section_seats_cache_key, student_timetable_cache_key,
    term_sections_cache_key, DeadLetterEvent, EnrollmentCommandEvent, EventCodecError,
    GateControlEvent, HotPathPubSubMessage, RevertEvent, SeatConfirmedEvent, SeatTentativeEvent,
    EVENT_SCHEMA_VERSION_V1, KAFKA_TOPIC_ENROLLMENT_COMMANDS, KAFKA_TOPIC_ENROLLMENT_DLQ,
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
