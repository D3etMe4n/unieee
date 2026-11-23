-- Phase 4: Academic Terms, Structure, Lecturers, and Students

CREATE TABLE academic_terms (
    id          UUID PRIMARY KEY,
    code        VARCHAR(32)  NOT NULL UNIQUE,
    name        VARCHAR(150) NOT NULL,
    start_date  DATE         NOT NULL,
    end_date    DATE         NOT NULL,
    status      VARCHAR(32)  NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_academic_terms_dates CHECK (end_date > start_date)
);

CREATE TABLE faculties (
    id          UUID PRIMARY KEY,
    code        VARCHAR(32)  NOT NULL UNIQUE,
    name        VARCHAR(150) NOT NULL,
    active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE majors (
    id                      UUID PRIMARY KEY,
    faculty_id              UUID         NOT NULL REFERENCES faculties (id),
    code                    VARCHAR(32)  NOT NULL UNIQUE,
    name                    VARCHAR(150) NOT NULL,
    total_required_credits  INT          NOT NULL,
    active                  BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_majors_required_credits CHECK (total_required_credits > 0)
);

CREATE INDEX idx_majors_faculty ON majors (faculty_id);

CREATE TABLE curricula (
    id          UUID PRIMARY KEY,
    major_id    UUID         NOT NULL REFERENCES majors (id),
    cohort_code VARCHAR(32)  NOT NULL,
    name        VARCHAR(150) NOT NULL,
    active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_curricula_major_cohort UNIQUE (major_id, cohort_code)
);

CREATE INDEX idx_curricula_major ON curricula (major_id);

CREATE TABLE curriculum_courses (
    id                   UUID PRIMARY KEY,
    curriculum_id        UUID        NOT NULL REFERENCES curricula (id) ON DELETE CASCADE,
    course_code          VARCHAR(32) NOT NULL,
    recommended_semester SMALLINT    NOT NULL,
    mandatory            BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_curriculum_courses_item UNIQUE (curriculum_id, course_code),
    CONSTRAINT chk_curriculum_courses_semester CHECK (recommended_semester BETWEEN 1 AND 16)
);

CREATE TABLE lecturers (
    id              UUID PRIMARY KEY,
    lecturer_code   VARCHAR(32)  NOT NULL UNIQUE,
    keycloak_sub    VARCHAR(64)  UNIQUE,
    full_name       VARCHAR(150) NOT NULL,
    email           VARCHAR(150) NOT NULL UNIQUE,
    department_code VARCHAR(32)  NOT NULL,
    academic_title  VARCHAR(64),
    status          VARCHAR(32)  NOT NULL DEFAULT 'ACTIVE',
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE students (
    id                 UUID PRIMARY KEY,
    student_code       VARCHAR(32)    NOT NULL UNIQUE,
    keycloak_sub       VARCHAR(64)    UNIQUE,
    full_name          VARCHAR(150)   NOT NULL,
    email              VARCHAR(150)   NOT NULL UNIQUE,
    cohort_code        VARCHAR(32)    NOT NULL,
    faculty_code       VARCHAR(32)    NOT NULL,
    major_code         VARCHAR(32)    NOT NULL,
    academic_status    VARCHAR(32)    NOT NULL DEFAULT 'ACTIVE',
    tuition_debt       NUMERIC(14, 2) NOT NULL DEFAULT 0.00,
    tuition_exempt     BOOLEAN        NOT NULL DEFAULT FALSE,
    enrollment_blocked BOOLEAN        NOT NULL DEFAULT FALSE,
    cumulative_gpa     NUMERIC(3, 2)  NOT NULL DEFAULT 0.00,
    earned_credits     INT            NOT NULL DEFAULT 0,
    created_at         TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_students_tuition_debt CHECK (tuition_debt >= 0),
    CONSTRAINT chk_students_gpa CHECK (cumulative_gpa >= 0.00 AND cumulative_gpa <= 4.00),
    CONSTRAINT chk_students_earned_credits CHECK (earned_credits >= 0)
);

CREATE INDEX idx_students_cohort_faculty ON students (cohort_code, faculty_code);
