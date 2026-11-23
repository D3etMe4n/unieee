-- Phase 4: Enrollments, Append-Only Event Sourcing Log, and Writer Revert Outbox

CREATE TABLE enrollments (
    id              UUID PRIMARY KEY,
    term_id         UUID         NOT NULL REFERENCES academic_terms (id),
    student_id      UUID         NOT NULL REFERENCES students (id),
    section_id      BIGINT       NOT NULL REFERENCES course_sections (id),
    course_id       UUID         NOT NULL REFERENCES courses (id),
    action_type     VARCHAR(32)  NOT NULL,
    status          VARCHAR(32)  NOT NULL,
    override_reason VARCHAR(500),
    acted_by        VARCHAR(128),
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Prevent duplicate active enrollments for the same student and course in the same term
CREATE UNIQUE INDEX uq_enrollments_active_student_course
    ON enrollments (term_id, student_id, course_id)
    WHERE status = 'CONFIRMED';

CREATE INDEX idx_enrollments_section_confirmed
    ON enrollments (section_id)
    WHERE status = 'CONFIRMED';

CREATE TABLE enrollment_events (
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
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uq_enrollment_events_event_id ON enrollment_events (event_id);
CREATE INDEX idx_events_term_seq ON enrollment_events (term_id, seq);
CREATE INDEX idx_events_student_created ON enrollment_events (student_id, created_at DESC);

CREATE TABLE writer_revert_outbox (
    event_id     UUID PRIMARY KEY,
    payload      JSONB       NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ
);

CREATE INDEX idx_writer_revert_outbox_unpublished
    ON writer_revert_outbox (created_at)
    WHERE published_at IS NULL;
