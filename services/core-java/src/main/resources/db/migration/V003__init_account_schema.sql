-- Bounded Context: account (Lecturers, Students with Financial/Academic Standing, Course Results)

CREATE SCHEMA IF NOT EXISTS account;

CREATE TABLE account.lecturers (
    id              UUID PRIMARY KEY,
    lecturer_code   VARCHAR(32)  NOT NULL UNIQUE,
    keycloak_sub    VARCHAR(64)  UNIQUE,
    full_name       VARCHAR(150) NOT NULL,
    email           VARCHAR(150) NOT NULL UNIQUE,
    department_code VARCHAR(32)  NOT NULL,
    academic_title  VARCHAR(64),
    status          VARCHAR(32)  NOT NULL DEFAULT 'ACTIVE',
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_lecturers_status CHECK (status IN ('ACTIVE', 'ON_LEAVE', 'LOCKED', 'RESIGNED'))
);

CREATE TABLE account.students (
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
    CONSTRAINT chk_students_status
        CHECK (academic_status IN ('ACTIVE', 'ON_LEAVE', 'SUSPENDED', 'LOCKED', 'GRADUATED', 'EXPELLED')),
    CONSTRAINT chk_students_tuition_debt CHECK (tuition_debt >= 0),
    CONSTRAINT chk_students_gpa CHECK (cumulative_gpa >= 0.00 AND cumulative_gpa <= 4.00),
    CONSTRAINT chk_students_earned_credits CHECK (earned_credits >= 0)
);

CREATE INDEX idx_students_cohort_faculty ON account.students (cohort_code, faculty_code);

CREATE TABLE account.student_course_results (
    id             UUID PRIMARY KEY,
    student_id     UUID          NOT NULL REFERENCES account.students (id) ON DELETE CASCADE,
    course_id      UUID          NOT NULL,
    term_id        UUID          NOT NULL,
    numeric_score  NUMERIC(4, 2),
    letter_grade   VARCHAR(8)    NOT NULL,
    grade_point    NUMERIC(3, 2) NOT NULL,
    passed         BOOLEAN       NOT NULL,
    attempt_number INT           NOT NULL DEFAULT 1,
    created_at     TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_results_student_course_attempt UNIQUE (student_id, course_id, attempt_number),
    CONSTRAINT chk_results_letter_grade
        CHECK (letter_grade IN ('A+', 'A', 'B+', 'B', 'C+', 'C', 'D+', 'D', 'F')),
    CONSTRAINT chk_results_grade_point CHECK (grade_point >= 0.00 AND grade_point <= 4.00),
    CONSTRAINT chk_results_attempt CHECK (attempt_number >= 1)
);

CREATE INDEX idx_results_student_course ON account.student_course_results (student_id, course_id, passed);
