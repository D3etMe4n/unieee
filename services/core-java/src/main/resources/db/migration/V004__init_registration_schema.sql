-- Bounded Context: registration (Enrollments, Append-Only Event Sourcing Log, Writer Revert Outbox)

CREATE SCHEMA IF NOT EXISTS registration;

CREATE TABLE registration.enrollments (
    id              UUID PRIMARY KEY,
    term_id         UUID         NOT NULL,
    student_id      UUID         NOT NULL,
    section_id      BIGINT       NOT NULL,
    course_id       UUID         NOT NULL,
    action_type     VARCHAR(32)  NOT NULL,
    status          VARCHAR(32)  NOT NULL,
    override_reason VARCHAR(500),
    acted_by        VARCHAR(128),
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_enrollments_action_type
        CHECK (action_type IN ('REGISTER', 'RETAKE', 'IMPROVE', 'ADMIN_ADD')),
    CONSTRAINT chk_enrollments_status
        CHECK (status IN ('CONFIRMED', 'DROPPED'))
);

-- Prevent duplicate active enrollments for the same student and course in the same term
CREATE UNIQUE INDEX uq_enrollments_active_student_course
    ON registration.enrollments (term_id, student_id, course_id)
    WHERE status = 'CONFIRMED';

CREATE INDEX idx_enrollments_section_confirmed
    ON registration.enrollments (section_id)
    WHERE status = 'CONFIRMED';

CREATE TABLE registration.enrollment_events (
    seq          BIGSERIAL PRIMARY KEY,
    event_id     UUID        NOT NULL,
    term_id      UUID        NOT NULL,
    student_id   UUID        NOT NULL,
    section_id   BIGINT      NOT NULL,
    event_type   VARCHAR(32) NOT NULL,
    status       VARCHAR(32) NOT NULL,
    reason_code  VARCHAR(128),
    audit_note   VARCHAR(500),
    acted_by     VARCHAR(128),
    engine_epoch BIGINT      NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_enrollment_events_type
        CHECK (event_type IN ('REGISTER', 'DROP', 'RETAKE', 'IMPROVE', 'ADMIN_ADD', 'ADMIN_DROP')),
    CONSTRAINT chk_enrollment_events_status
        CHECK (status IN ('CONFIRMED', 'REJECTED', 'REVERTED'))
);

CREATE UNIQUE INDEX uq_enrollment_events_event_id ON registration.enrollment_events (event_id);
CREATE INDEX idx_events_term_seq ON registration.enrollment_events (term_id, seq);
CREATE INDEX idx_events_student_created ON registration.enrollment_events (student_id, created_at DESC);

CREATE TABLE registration.writer_revert_outbox (
    event_id     UUID PRIMARY KEY,
    payload      JSONB       NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ
);

CREATE INDEX idx_writer_revert_outbox_unpublished
    ON registration.writer_revert_outbox (created_at)
    WHERE published_at IS NULL;
