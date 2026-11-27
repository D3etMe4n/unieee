import http from "k6/http";
import { check } from "k6";
import { Counter, Trend } from "k6/metrics";
import { resolveRuntimeConfig } from "../lib/profiles.js";
import {
  authHeaders,
  loadBenchContext,
  pickStudent,
  recordUnexpectedStatus,
  stableStudentForVu,
} from "../lib/common.js";

const cfg = resolveRuntimeConfig("mixed");
const ctx = loadBenchContext();

if (cfg.allowRateLimit) {
  http.setResponseCallback(http.expectedStatuses(200, 202, 404, 409, 422, 429));
} else {
  http.setResponseCallback(http.expectedStatuses(200, 202, 404, 409, 422));
}

const mixedReadTrend = new Trend("unieee_mixed_read_ms");
const mixedWriteTrend = new Trend("unieee_mixed_write_ms");
const mixedStatusTrend = new Trend("unieee_mixed_status_ms");
const mixedAcceptedCounter = new Counter("unieee_mixed_accepted_total");
const mixedRejectedCounter = new Counter("unieee_mixed_business_rejected_total");

const readVus = Math.max(2, Math.round(cfg.vus * 0.6));
const writeVus = Math.max(2, cfg.vus - readVus);

let heldChurnSection = null;

export const options = {
  scenarios: {
    mixed_reads: {
      executor: "constant-vus",
      vus: readVus,
      duration: cfg.duration,
      exec: "runReads",
    },
    mixed_writes: {
      executor: "constant-vus",
      vus: writeVus,
      duration: cfg.duration,
      exec: "runWrites",
    },
  },
  thresholds: {
    unieee_unexpected_error_rate: ["rate<0.001"],
    unieee_mixed_read_ms: ["p(95)<50", "p(99)<150"],
    unieee_mixed_write_ms: ["p(95)<50", "p(99)<150"],
  },
};

export function runReads() {
  const student = pickStudent(ctx);
  const url =
    Math.random() < 0.70
      ? `${cfg.engineUrl}/api/v1/enrollments/seats?termId=${ctx.termId}`
      : `${cfg.engineUrl}/api/v1/enrollments/me?termId=${ctx.termId}`;

  const res = http.get(url, {
    headers: authHeaders(student.token),
    tags: { endpoint: "mixed_read" },
  });
  mixedReadTrend.add(res.timings.duration);
  recordUnexpectedStatus(res, null, cfg.allowRateLimit);
  check(res, {
    "mixed read 200": (r) => r.status === 200 || (cfg.allowRateLimit && r.status === 429),
  });
}

export function runWrites() {
  const student = stableStudentForVu(ctx);
  const roll = Math.random();

  // Keep continuous churn on churnSectionIds while also driving hot-section contention
  if (roll < 0.40) {
    const churnPool = ctx.churnSectionIds && ctx.churnSectionIds.length > 0
      ? ctx.churnSectionIds
      : ctx.normalSectionIds;
    if (heldChurnSection === null) {
      const sectionId = churnPool[(__VU - 1 + __ITER) % churnPool.length];
      const res = http.post(
        `${cfg.engineUrl}/api/v1/enrollments/register`,
        JSON.stringify({ sectionId: sectionId, action: "REGISTER" }),
        {
          headers: authHeaders(student.token, {
            "Content-Type": "application/json",
            "Idempotency-Key": `mix-creg-${student.code}-${__VU}-${__ITER}`,
          }),
          tags: { endpoint: "mixed_write_churn_reg" },
        }
      );
      mixedWriteTrend.add(res.timings.duration);
      recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
      if (res.status === 202) {
        heldChurnSection = sectionId;
        mixedAcceptedCounter.add(1);
      } else if (res.status === 409 || res.status === 422) {
        mixedRejectedCounter.add(1);
      }
    } else {
      const sectionId = heldChurnSection;
      const res = http.post(
        `${cfg.engineUrl}/api/v1/enrollments/drop`,
        JSON.stringify({ sectionId: sectionId }),
        {
          headers: authHeaders(student.token, {
            "Content-Type": "application/json",
            "Idempotency-Key": `mix-cdrop-${student.code}-${__VU}-${__ITER}`,
          }),
          tags: { endpoint: "mixed_write_churn_drop" },
        }
      );
      mixedWriteTrend.add(res.timings.duration);
      recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
      heldChurnSection = null;
      if (res.status === 202) {
        mixedAcceptedCounter.add(1);
      } else if (res.status === 409 || res.status === 422) {
        mixedRejectedCounter.add(1);
      }
    }
    return;
  }

  const allTargets = ctx.hotSectionIds.concat(ctx.normalSectionIds);
  const pool = Math.random() < 0.65 ? ctx.hotSectionIds : allTargets;
  const sectionId = pool[Math.floor(Math.random() * pool.length)];

  if (Math.random() < 0.80) {
    const res = http.post(
      `${cfg.engineUrl}/api/v1/enrollments/register`,
      JSON.stringify({ sectionId: sectionId, action: "REGISTER" }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `mix-reg-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "mixed_register" },
      }
    );
    mixedWriteTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
    check(res, {
      "mixed register valid": (r) =>
        r.status === 202 || r.status === 409 || r.status === 422 || (cfg.allowRateLimit && r.status === 429),
    });
    if (res.status === 202) {
      mixedAcceptedCounter.add(1);
      try {
        const body = JSON.parse(res.body);
        if (body && body.eventId && Math.random() < 0.25) {
          const stRes = http.get(`${cfg.engineUrl}/api/v1/enrollments/status/${body.eventId}`, {
            headers: authHeaders(student.token),
            tags: { endpoint: "mixed_status" },
          });
          mixedStatusTrend.add(stRes.timings.duration);
          recordUnexpectedStatus(stRes, [404], cfg.allowRateLimit);
        }
      } catch (_) {}
    } else if (res.status === 409 || res.status === 422) {
      mixedRejectedCounter.add(1);
    }
  } else {
    const res = http.post(
      `${cfg.engineUrl}/api/v1/enrollments/drop`,
      JSON.stringify({ sectionId: sectionId }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `mix-drop-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "mixed_drop" },
      }
    );
    mixedWriteTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
    if (res.status === 202) {
      mixedAcceptedCounter.add(1);
    } else if (res.status === 409 || res.status === 422) {
      mixedRejectedCounter.add(1);
    }
  }
}
