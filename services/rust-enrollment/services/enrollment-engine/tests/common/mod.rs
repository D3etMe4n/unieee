#![allow(dead_code)]

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use enrollment_engine::core::{
    ActiveEnrollment, CommandPublisher, CoursePrerequisiteEdge, EnrollmentEngine,
    FallbackEventLogSource, JwksFetcher, KafkaReplayBatch, KafkaReplaySource, NodeRole,
    PreloadDataset, PreloadSource, PublishError, ReplayEventRecord, RevertDedupStore,
    SeatTentativeCoalescerHandle, SectionMeta, StudentCourseAttempt, StudentState,
    TentativeSeatSink,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use p256::pkcs8::EncodePrivateKey as _;
use rsa::traits::PublicKeyParts as _;
use rsa::RsaPrivateKey;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use unieee_domain::{
    CourseId, CreditLimitPolicy, EnrollmentAction, EventId, GateState, GradeRank, PolicyId,
    PolicySnapshotPayload, PrerequisitePolicy, RegistrationWindowSnapshot, RetakeImprovePolicy,
    ScheduleBitmask, ScheduleConflictPolicy, SectionId, SectionStatus, StudentId, TermId,
    TuitionHoldPolicy, WindowId, WindowStatus,
};
use unieee_events::{EnrollmentCommandEvent, SeatTentativeEvent};

pub const ALL_TWENTY_WEEKS: &str = "11111111111111111111";

pub struct MockCommandPublisher {
    pub fail: AtomicBool,
    pub fail_indeterminate: AtomicBool,
    pub pause_entered: Arc<tokio::sync::Notify>,
    pub pause_release: Arc<tokio::sync::Notify>,
    pub pause_next: AtomicBool,
    pub published: Mutex<Vec<EnrollmentCommandEvent>>,
}

impl Default for MockCommandPublisher {
    fn default() -> Self {
        Self {
            fail: AtomicBool::new(false),
            fail_indeterminate: AtomicBool::new(false),
            pause_entered: Arc::new(tokio::sync::Notify::new()),
            pause_release: Arc::new(tokio::sync::Notify::new()),
            pause_next: AtomicBool::new(false),
            published: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl CommandPublisher for MockCommandPublisher {
    async fn publish_command(&self, event: &EnrollmentCommandEvent) -> Result<(), PublishError> {
        if self.pause_next.swap(false, Ordering::AcqRel) {
            self.pause_entered.notify_waiters();
            self.pause_release.notified().await;
        }
        if self.fail_indeterminate.load(Ordering::Acquire) {
            return Err(PublishError::Indeterminate(
                "simulated Kafka MessageTimedOut".to_owned(),
            ));
        }
        if self.fail.load(Ordering::Acquire) {
            return Err(PublishError::Definite("simulated Kafka outage".to_owned()));
        }
        self.published.lock().await.push(event.clone());
        Ok(())
    }
}

#[derive(Default)]
pub struct MockTentativeSink {
    pub fail: AtomicBool,
    pub batches: Mutex<Vec<Vec<SeatTentativeEvent>>>,
}

#[async_trait]
impl TentativeSeatSink for MockTentativeSink {
    async fn publish_tentative_batch(&self, events: &[SeatTentativeEvent]) -> Result<(), String> {
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated Redis outage".to_owned());
        }
        self.batches.lock().await.push(events.to_vec());
        Ok(())
    }
}

#[derive(Default)]
pub struct MockRevertDedupStore {
    pub fail: AtomicBool,
    pub recorded: Mutex<Vec<(EventId, u64)>>,
}

#[async_trait]
impl RevertDedupStore for MockRevertDedupStore {
    async fn mark_reverted(&self, event_id: EventId, ttl_secs: u64) -> Result<(), String> {
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated Redis write failure".to_owned());
        }
        self.recorded.lock().await.push((event_id, ttl_secs));
        Ok(())
    }
}

pub struct MockPreloadSource {
    pub calls: AtomicUsize,
    pub dataset: Mutex<Option<PreloadDataset>>,
    pub fail: AtomicBool,
}

impl MockPreloadSource {
    pub fn new(dataset: PreloadDataset) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            dataset: Mutex::new(Some(dataset)),
            fail: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl PreloadSource for MockPreloadSource {
    async fn load_term_dataset(
        &self,
        _term_id: Option<TermId>,
        _term_code: Option<&str>,
    ) -> Result<PreloadDataset, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated PG error".to_owned());
        }
        self.dataset
            .lock()
            .await
            .clone()
            .ok_or_else(|| "missing preload dataset".to_owned())
    }
}

#[derive(Default)]
pub struct MockKafkaReplaySource {
    pub calls: AtomicUsize,
    pub fail: AtomicBool,
    pub batch: Mutex<KafkaReplayBatch>,
}

#[async_trait]
impl KafkaReplaySource for MockKafkaReplaySource {
    async fn replay_since(
        &self,
        _term_id: TermId,
        _from_timestamp_ms: i64,
    ) -> Result<KafkaReplayBatch, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated Kafka replay error".to_owned());
        }
        Ok(self.batch.lock().await.clone())
    }
}

#[derive(Default)]
pub struct MockFallbackEventLogSource {
    pub calls: AtomicUsize,
    pub events: Mutex<Vec<ReplayEventRecord>>,
}

#[async_trait]
impl FallbackEventLogSource for MockFallbackEventLogSource {
    async fn fetch_events_page(
        &self,
        term_id: TermId,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<ReplayEventRecord>, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let guard = self.events.lock().await;
        let filtered: Vec<_> = guard
            .iter()
            .filter(|e| e.term_id == term_id && e.seq > after_seq)
            .take(limit.max(1) as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }
}

pub struct MockJwksFetcher {
    pub calls: AtomicUsize,
    pub delay_ms: u64,
    pub body: Mutex<Result<String, String>>,
}

impl MockJwksFetcher {
    pub fn with_ok(jwks_json: String) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            delay_ms: 0,
            body: Mutex::new(Ok(jwks_json)),
        }
    }
}

#[async_trait]
impl JwksFetcher for MockJwksFetcher {
    async fn fetch_jwks_json(&self) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }
        self.body.lock().await.clone()
    }
}

pub fn make_default_policy(term_id: TermId, now: DateTime<Utc>) -> PolicySnapshotPayload {
    let win_id = WindowId::new_v4();
    PolicySnapshotPayload {
        schema_version: 1,
        term_id,
        term_code: "2026-1".to_owned(),
        policy_id: Some(PolicyId::new_v4()),
        policy_code: "DEFAULT_UNI".to_owned(),
        snapshot_version: 1,
        exported_at: now,
        credit_limit: CreditLimitPolicy {
            enforce_credit_limit: true,
            min_credits: 0,
            max_credits: 18,
            gpa_bonus_threshold: Some(Decimal::new(350, 2)),
            max_credits_with_bonus: Some(24),
            count_retake_in_credit_limit: true,
        },
        tuition_hold: TuitionHoldPolicy {
            block_on_tuition_debt: true,
            max_allowed_debt: Decimal::new(50_000, 2),
            exempt_cohorts: vec!["HONORS".to_owned()],
            exempt_faculties: vec!["MEDICINE".to_owned()],
        },
        prerequisite: PrerequisitePolicy {
            enforce_prerequisite: true,
            min_passing_grade: GradeRank::D,
            allow_concurrent_for_retake: false,
            exempt_cohorts: vec!["INTL".to_owned()],
        },
        retake_improve: RetakeImprovePolicy {
            allow_improve_passed_course: true,
            max_grade_to_improve: GradeRank::C,
            max_improve_attempts: 1,
        },
        schedule_conflict: ScheduleConflictPolicy {
            strict_no_overlap: true,
            allow_buffer_gap_periods: 0,
        },
        extended_rules: BTreeMap::new(),
        windows: vec![RegistrationWindowSnapshot {
            id: Some(win_id),
            name: "Wave 1 - All".to_owned(),
            status: WindowStatus::Active,
            allowed_cohorts: Vec::new(),
            allowed_faculties: Vec::new(),
            allowed_actions: vec![
                EnrollmentAction::Register,
                EnrollmentAction::Drop,
                EnrollmentAction::Retake,
                EnrollmentAction::Improve,
            ],
            start_at: now - ChronoDuration::hours(1),
            end_at: now + ChronoDuration::hours(5),
        }],
    }
}

#[allow(clippy::too_many_arguments)]
pub fn make_section(
    term_id: TermId,
    section_id: u64,
    course_id: CourseId,
    course_code: &str,
    credits: i32,
    max_capacity: i32,
    enrolled_count: i32,
    day: u8,
    start_period: u8,
    period_count: u8,
) -> SectionMeta {
    SectionMeta {
        section_id: SectionId::new(section_id),
        term_id,
        course_id,
        course_code: course_code.to_owned(),
        section_code: format!("{course_code}-{section_id}"),
        credits,
        max_capacity,
        enrolled_count,
        status: SectionStatus::Open,
        bitmask: ScheduleBitmask::from_slot_parts(
            day,
            start_period,
            period_count,
            ALL_TWENTY_WEEKS,
        )
        .unwrap(),
    }
}

pub fn make_student(
    student_id: StudentId,
    code: &str,
    sub: Option<&str>,
    cohort: &str,
    faculty: &str,
) -> StudentState {
    StudentState {
        student_id,
        student_code: code.to_owned(),
        keycloak_sub: sub.map(ToOwned::to_owned),
        cohort_code: cohort.to_owned(),
        faculty_code: faculty.to_owned(),
        academic_status: "ACTIVE".to_owned(),
        tuition_debt: Decimal::ZERO,
        tuition_exempt: false,
        enrollment_blocked: false,
        cumulative_gpa: Decimal::new(300, 2),
        current_credits: 0,
        schedule_bitmask: ScheduleBitmask::empty(),
        active_enrollments: Vec::<ActiveEnrollment>::new(),
        course_results: Vec::<StudentCourseAttempt>::new(),
        processed_event_ids: HashSet::new(),
        reverted_event_ids: HashSet::new(),
    }
}

pub fn make_dataset(
    term_id: TermId,
    sections: Vec<SectionMeta>,
    prerequisites: Vec<CoursePrerequisiteEdge>,
    students: Vec<StudentState>,
    now: DateTime<Utc>,
) -> PreloadDataset {
    let policy = make_default_policy(term_id, now);
    let active_window_id = policy.windows.first().and_then(|w| w.id);
    PreloadDataset {
        term_id,
        term_code: "2026-1".to_owned(),
        checkpoint_seq: 100,
        engine_epoch: 1,
        gate_state: GateState::Open,
        active_window_id,
        policy,
        sections,
        prerequisites,
        students,
        created_at: now,
    }
}

pub fn make_ready_engine(
    dataset: PreloadDataset,
    role: NodeRole,
) -> (
    Arc<EnrollmentEngine>,
    Arc<MockCommandPublisher>,
    Arc<MockRevertDedupStore>,
) {
    let pub_mock = Arc::new(MockCommandPublisher::default());
    let dedup_mock = Arc::new(MockRevertDedupStore::default());
    let engine = Arc::new(EnrollmentEngine::new_awaiting_preload(
        role,
        Arc::clone(&pub_mock) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::clone(&dedup_mock) as Arc<dyn RevertDedupStore>,
    ));
    engine.hydrate_from_dataset(dataset, role).unwrap();
    engine.set_readiness(enrollment_engine::core::EngineReadiness::Ready);
    (engine, pub_mock, dedup_mock)
}

#[derive(Serialize)]
struct JwtClaimsPayload<'a> {
    sub: &'a str,
    iss: &'a str,
    aud: &'a str,
    exp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    student_id: Option<String>,
    realm_access: RealmRolesPayload<'a>,
}

#[derive(Serialize)]
struct RealmRolesPayload<'a> {
    roles: &'a [&'a str],
}

pub struct RsaTestKeypair {
    pub kid: String,
    pub jwk_json: serde_json::Value,
    pub encoding_key: EncodingKey,
}

impl RsaTestKeypair {
    pub fn generate(kid: &str) -> Self {
        let mut rng = rand::thread_rng();
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pub_key = priv_key.to_public_key();
        let n = URL_SAFE_NO_PAD.encode(pub_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(pub_key.e().to_bytes_be());
        let pem = priv_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
        let jwk_json = serde_json::json!({
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": kid,
            "n": n,
            "e": e
        });
        Self {
            kid: kid.to_owned(),
            jwk_json,
            encoding_key,
        }
    }

    pub fn sign_jwt(
        &self,
        sub: &str,
        iss: &str,
        aud: &str,
        exp: i64,
        roles: &[&str],
        student_id: Option<StudentId>,
    ) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        let claims = JwtClaimsPayload {
            sub,
            iss,
            aud,
            exp,
            preferred_username: Some("student01"),
            student_id: student_id.map(|id| id.to_string()),
            realm_access: RealmRolesPayload { roles },
        };
        encode(&header, &claims, &self.encoding_key).unwrap()
    }
}

pub struct EcP256TestKeypair {
    pub kid: String,
    pub jwk_json: serde_json::Value,
    pub encoding_key: EncodingKey,
}

impl EcP256TestKeypair {
    pub fn generate(kid: &str) -> Self {
        let mut rng = rand::thread_rng();
        let secret = p256::SecretKey::random(&mut rng);
        let pub_point = secret.public_key();
        let encoded_pt = p256::EncodedPoint::from(pub_point);
        let x = URL_SAFE_NO_PAD.encode(encoded_pt.x().unwrap());
        let y = URL_SAFE_NO_PAD.encode(encoded_pt.y().unwrap());
        let pem = secret.to_pkcs8_pem(p256::pkcs8::LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_ec_pem(pem.as_bytes()).unwrap();
        let jwk_json = serde_json::json!({
            "kty": "EC",
            "use": "sig",
            "crv": "P-256",
            "alg": "ES256",
            "kid": kid,
            "x": x,
            "y": y
        });
        Self {
            kid: kid.to_owned(),
            jwk_json,
            encoding_key,
        }
    }

    pub fn sign_jwt(
        &self,
        sub: &str,
        iss: &str,
        aud: &str,
        exp: i64,
        roles: &[&str],
        student_id: Option<StudentId>,
    ) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.kid.clone());
        let claims = JwtClaimsPayload {
            sub,
            iss,
            aud,
            exp,
            preferred_username: Some("student02"),
            student_id: student_id.map(|id| id.to_string()),
            realm_access: RealmRolesPayload { roles },
        };
        encode(&header, &claims, &self.encoding_key).unwrap()
    }
}
