-- Bounded Context: registration (Phase 5: Wish Surveys, Multi-University Policy Engine & Registration Windows)

CREATE TABLE registration.wish_survey_campaigns (
    id              UUID PRIMARY KEY,
    term_id         UUID         NOT NULL,
    title           VARCHAR(255) NOT NULL,
    description     VARCHAR(1000),
    allowed_cohorts JSONB        NOT NULL DEFAULT '[]'::jsonb,
    open_at         TIMESTAMPTZ  NOT NULL,
    close_at        TIMESTAMPTZ  NOT NULL,
    status          VARCHAR(32)  NOT NULL DEFAULT 'DRAFT',
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_wish_survey_campaigns_time
        CHECK (close_at > open_at),
    CONSTRAINT chk_wish_survey_campaigns_status
        CHECK (status IN ('DRAFT', 'OPEN', 'CLOSED'))
);

CREATE INDEX idx_wish_survey_campaigns_term_status
    ON registration.wish_survey_campaigns (term_id, status);

CREATE TABLE registration.wish_survey_entries (
    id          UUID PRIMARY KEY,
    campaign_id UUID         NOT NULL REFERENCES registration.wish_survey_campaigns (id) ON DELETE CASCADE,
    student_id  UUID         NOT NULL,
    course_id   UUID         NOT NULL,
    priority    SMALLINT     NOT NULL DEFAULT 1,
    note        VARCHAR(500),
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_survey_student_course
        UNIQUE (campaign_id, student_id, course_id),
    CONSTRAINT chk_wish_survey_entries_priority
        CHECK (priority BETWEEN 1 AND 5)
);

CREATE INDEX idx_survey_campaign_course
    ON registration.wish_survey_entries (campaign_id, course_id);

CREATE INDEX idx_survey_student_campaign
    ON registration.wish_survey_entries (student_id, campaign_id);

CREATE TABLE registration.enrollment_policies (
    id                           UUID PRIMARY KEY,
    term_id                      UUID           NOT NULL,
    policy_code                  VARCHAR(64)    NOT NULL,
    name                         VARCHAR(255)   NOT NULL,
    min_credits                  INTEGER        NOT NULL DEFAULT 0,
    max_credits                  INTEGER        NOT NULL DEFAULT 24,
    gpa_bonus_threshold          NUMERIC(3, 2),
    max_credits_with_bonus       INTEGER,
    enforce_credit_limit         BOOLEAN        NOT NULL DEFAULT TRUE,
    block_on_tuition_debt        BOOLEAN        NOT NULL DEFAULT TRUE,
    max_allowed_debt             NUMERIC(14, 2) NOT NULL DEFAULT 0.00,
    enforce_prerequisite         BOOLEAN        NOT NULL DEFAULT TRUE,
    min_passing_grade            VARCHAR(4)     NOT NULL DEFAULT 'D',
    allow_concurrent_for_retake  BOOLEAN        NOT NULL DEFAULT FALSE,
    allow_improve_passed_course  BOOLEAN        NOT NULL DEFAULT TRUE,
    max_grade_to_improve         VARCHAR(4)     NOT NULL DEFAULT 'C',
    max_improve_attempts         INTEGER        NOT NULL DEFAULT 2,
    count_retake_in_credit_limit BOOLEAN        NOT NULL DEFAULT TRUE,
    strict_no_overlap            BOOLEAN        NOT NULL DEFAULT TRUE,
    allow_buffer_gap_periods     SMALLINT       NOT NULL DEFAULT 0,
    extended_rules               JSONB          NOT NULL DEFAULT '{}'::jsonb,
    active                       BOOLEAN        NOT NULL DEFAULT FALSE,
    version                      BIGINT         NOT NULL DEFAULT 0,
    created_at                   TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    updated_at                   TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_enrollment_policies_term_code
        UNIQUE (term_id, policy_code),
    CONSTRAINT chk_enrollment_policies_credits
        CHECK (
            min_credits >= 0
            AND max_credits >= min_credits
            AND (max_credits_with_bonus IS NULL OR max_credits_with_bonus >= max_credits)
        ),
    CONSTRAINT chk_enrollment_policies_gpa
        CHECK (gpa_bonus_threshold IS NULL OR (gpa_bonus_threshold >= 0.00 AND gpa_bonus_threshold <= 4.00)),
    CONSTRAINT chk_enrollment_policies_debt
        CHECK (max_allowed_debt >= 0.00),
    CONSTRAINT chk_enrollment_policies_min_passing_grade
        CHECK (min_passing_grade IN ('A+', 'A', 'B+', 'B', 'C+', 'C', 'D+', 'D')),
    CONSTRAINT chk_enrollment_policies_max_improve_grade
        CHECK (max_grade_to_improve IN ('A+', 'A', 'B+', 'B', 'C+', 'C', 'D+', 'D')),
    CONSTRAINT chk_enrollment_policies_improve_attempts
        CHECK (max_improve_attempts >= 0),
    CONSTRAINT chk_enrollment_policies_buffer_gap
        CHECK (allow_buffer_gap_periods >= 0 AND allow_buffer_gap_periods <= 4)
);

CREATE UNIQUE INDEX uq_enrollment_policies_active_term
    ON registration.enrollment_policies (term_id)
    WHERE active = TRUE;

CREATE INDEX idx_enrollment_policies_term
    ON registration.enrollment_policies (term_id);

CREATE TABLE registration.enrollment_policy_snapshots (
    id               UUID PRIMARY KEY,
    term_id          UUID        NOT NULL,
    policy_id        UUID        NOT NULL REFERENCES registration.enrollment_policies (id) ON DELETE CASCADE,
    snapshot_version INTEGER     NOT NULL,
    checksum_sha256  VARCHAR(64) NOT NULL,
    payload          JSONB       NOT NULL,
    exported_by      VARCHAR(128),
    exported_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_policy_snapshots_term_version
        UNIQUE (term_id, snapshot_version)
);

CREATE INDEX idx_policy_snapshots_term_latest
    ON registration.enrollment_policy_snapshots (term_id, snapshot_version DESC);

CREATE TABLE registration.registration_windows (
    id                UUID PRIMARY KEY,
    term_id           UUID         NOT NULL,
    name              VARCHAR(255) NOT NULL,
    allowed_cohorts   JSONB        NOT NULL DEFAULT '[]'::jsonb,
    allowed_faculties JSONB        NOT NULL DEFAULT '[]'::jsonb,
    allowed_actions   JSONB        NOT NULL DEFAULT '["REGISTER"]'::jsonb,
    start_at          TIMESTAMPTZ  NOT NULL,
    end_at            TIMESTAMPTZ  NOT NULL,
    status            VARCHAR(32)  NOT NULL DEFAULT 'SCHEDULED',
    created_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_registration_windows_time
        CHECK (end_at > start_at),
    CONSTRAINT chk_registration_windows_status
        CHECK (status IN ('SCHEDULED', 'ACTIVE', 'PAUSED', 'CLOSED'))
);

CREATE INDEX idx_registration_windows_term_status
    ON registration.registration_windows (term_id, status, start_at);
