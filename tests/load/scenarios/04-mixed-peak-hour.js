import http from "k6/http";
import { check } from "k6";
import { Rate, Trend } from "k6/metrics";
import { resolveRuntimeConfig } from "../lib/profiles.js";
import {
  authHeaders,
  loadBenchContext,
  pickStudent,
  recordCacheHeader,
  recordUnexpectedStatus,
  stableStudentForVu,
} from "../lib/common.js";

http.setResponseCallback(http.expectedStatuses(200, 202, 409, 422, 429));

const cfg = resolveRuntimeConfig();
const ctx = loadBenchContext();
const isChaos = (__ENV.BENCH_CHAOS || "none").toLowerCase() !== "none";

const warmReadTrend = new Trend("unieee_mixed_warm_read_ms");
const hotReadTrend = new Trend("unieee_mixed_hot_read_ms");
const hotWriteTrend = new Trend("unieee_mixed_hot_write_ms");
const aiAdvisorTrend = new Trend("unieee_mixed_ai_advisor_ms");
const chaosFailover5xxRate = new Rate("unieee_chaos_failover_5xx_rate");

function recordMixedStatus(res, allowedExtraStatuses) {
  if (isChaos && (res.status === 502 || res.status === 503)) {
    chaosFailover5xxRate.add(true);
    return false;
  }
  chaosFailover5xxRate.add(false);
  return recordUnexpectedStatus(res, allowedExtraStatuses);
}

const totalVus = Math.max(cfg.vus, 10);
const readVus = Math.max(4, Math.round(totalVus * 0.65));
const writeVus = Math.max(4, Math.round(totalVus * 0.30));
const aiVus = Math.max(1, totalVus - readVus - writeVus);

export const options = {
  insecureSkipTLSVerify: true,
  scenarios: {
    peak_reads: {
      executor: "constant-vus",
      exec: "runReads",
      vus: readVus,
      duration: cfg.duration,
    },
    peak_writes: {
      executor: "constant-vus",
      exec: "runWrites",
      vus: writeVus,
      duration: cfg.duration,
    },
    ai_advisor: {
      executor: "constant-vus",
      exec: "runAiAdvisor",
      vus: aiVus,
      duration: cfg.duration,
    },
  },
  thresholds: isChaos
    ? {
        unieee_unexpected_error_rate: ["rate<0.001"],
      }
    : {
        unieee_unexpected_error_rate: ["rate<0.001"],
        unieee_mixed_hot_write_ms: ["p(95)<50", "p(99)<150"],
        unieee_mixed_hot_read_ms: ["p(95)<50", "p(99)<150"],
      },
};

export function runReads() {
  const student = pickStudent(ctx);
  const r = Math.random();
  if (r < 0.45) {
    const res = http.get(`${cfg.baseUrl}/api/v1/enrollments/seats?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "mixed_hot_seats" },
    });
    hotReadTrend.add(res.timings.duration);
    recordCacheHeader(res);
    recordMixedStatus(res);
    check(res, { "mixed seats 200": (x) => x.status === 200 });
  } else if (r < 0.75) {
    const res = http.get(`${cfg.baseUrl}/api/v1/public/sections?termId=${ctx.termId}`, {
      tags: { endpoint: "mixed_public_sections" },
    });
    warmReadTrend.add(res.timings.duration);
    recordCacheHeader(res);
    recordMixedStatus(res);
    check(res, { "mixed public sections 200": (x) => x.status === 200 });
  } else if (r < 0.90) {
    const res = http.get(`${cfg.baseUrl}/api/v1/enrollments/me?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "mixed_hot_me" },
    });
    hotReadTrend.add(res.timings.duration);
    recordMixedStatus(res);
    check(res, { "mixed me 200": (x) => x.status === 200 });
  } else {
    const res = http.get(`${cfg.baseUrl}/api/v1/students/me/timetable?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "mixed_timetable" },
    });
    warmReadTrend.add(res.timings.duration);
    recordMixedStatus(res);
    check(res, { "mixed timetable 200": (x) => x.status === 200 });
  }
}

export function runWrites() {
  const student = stableStudentForVu(ctx);
  const allTargets = ctx.hotSectionIds.concat(ctx.normalSectionIds);
  const sectionId =
    Math.random() < 0.55
      ? ctx.hotSectionIds[Math.floor(Math.random() * ctx.hotSectionIds.length)]
      : allTargets[Math.floor(Math.random() * allTargets.length)];

  if (Math.random() < 0.80) {
    const res = http.post(
      `${cfg.baseUrl}/api/v1/enrollments/register`,
      JSON.stringify({ sectionId: sectionId, action: "REGISTER" }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `k6-mix-reg-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "mixed_register" },
      }
    );
    hotWriteTrend.add(res.timings.duration);
    recordMixedStatus(res, [409, 422]);
    check(res, {
      "mixed register valid": (x) => x.status === 202 || x.status === 409 || x.status === 422,
    });
  } else {
    const res = http.post(
      `${cfg.baseUrl}/api/v1/enrollments/drop`,
      JSON.stringify({ sectionId: sectionId }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `k6-mix-drop-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "mixed_drop" },
      }
    );
    hotWriteTrend.add(res.timings.duration);
    recordMixedStatus(res, [409, 422]);
    check(res, {
      "mixed drop valid": (x) => x.status === 202 || x.status === 409 || x.status === 422,
    });
  }
}

export function runAiAdvisor() {
  const student = pickStudent(ctx);
  const res = http.post(
    `${cfg.baseUrl}/api/v1/ai/advisor/chat`,
    JSON.stringify({
      termId: ctx.termId,
      message: "Suggest a conflict-free schedule around 9 credits.",
      targetCredits: 9,
      avoidDaysOfWeek: [7],
      avoidMorningPeriods: false,
      avoidEveningPeriods: false,
    }),
    {
      headers: authHeaders(student.token, {
        "Content-Type": "application/json",
      }),
      tags: { endpoint: "mixed_ai_advisor" },
    }
  );
  aiAdvisorTrend.add(res.timings.duration);
  // 429 is expected protection from Nginx ai_limit or per-student AI rate limiter
  recordMixedStatus(res, [429]);
  check(res, {
    "ai advisor 200 or 429": (x) => x.status === 200 || x.status === 429,
  });
}
