import http from "k6/http";
import { check } from "k6";
import { Counter, Trend } from "k6/metrics";
import { resolveRuntimeConfig } from "../lib/profiles.js";
import {
  authHeaders,
  loadBenchContext,
  recordUnexpectedStatus,
  stableStudentForVu,
} from "../lib/common.js";

const cfg = resolveRuntimeConfig("write");
const ctx = loadBenchContext();

if (cfg.allowRateLimit) {
  http.setResponseCallback(http.expectedStatuses(200, 202, 404, 409, 422, 429));
} else {
  http.setResponseCallback(http.expectedStatuses(200, 202, 404, 409, 422));
}

const registerTrend = new Trend("unieee_engine_register_ms");
const dropTrend = new Trend("unieee_engine_drop_ms");
const statusTrend = new Trend("unieee_engine_status_ms");
const acceptedCounter = new Counter("unieee_engine_accepted_total");
const businessRejectedCounter = new Counter("unieee_engine_business_rejected_total");

export const options = Object.assign(
  {
    thresholds: {
      unieee_unexpected_error_rate: ["rate<0.001"],
      unieee_engine_register_ms: ["p(95)<50", "p(99)<150"],
    },
  },
  cfg.targetRps > 0
    ? {
        scenarios: {
          engine_write: {
            executor: "constant-arrival-rate",
            rate: cfg.targetRps,
            timeUnit: "1s",
            duration: cfg.duration,
            preAllocatedVUs: cfg.vus,
            maxVUs: Math.max(cfg.vus * 2, 40),
          },
        },
      }
    : {
        vus: cfg.vus,
        duration: cfg.duration,
      }
);

export default function () {
  const student = stableStudentForVu(ctx);
  const allTargets = ctx.hotSectionIds.concat(ctx.normalSectionIds);
  const useHot = Math.random() < 0.70;
  const pool = useHot ? ctx.hotSectionIds : allTargets;
  const sectionId = pool[Math.floor(Math.random() * pool.length)];

  const roll = Math.random();
  if (roll < 0.80) {
    const res = http.post(
      `${cfg.engineUrl}/api/v1/enrollments/register`,
      JSON.stringify({ sectionId: sectionId, action: "REGISTER" }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `rb-reg-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "engine_register" },
      }
    );
    registerTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
    check(res, {
      "register valid status": (r) =>
        r.status === 202 || r.status === 409 || r.status === 422 || (cfg.allowRateLimit && r.status === 429),
    });
    if (res.status === 202) {
      acceptedCounter.add(1);
      try {
        const body = JSON.parse(res.body);
        if (body && body.eventId && Math.random() < 0.25) {
          const stRes = http.get(`${cfg.engineUrl}/api/v1/enrollments/status/${body.eventId}`, {
            headers: authHeaders(student.token),
            tags: { endpoint: "engine_status" },
          });
          statusTrend.add(stRes.timings.duration);
          recordUnexpectedStatus(stRes, [404], cfg.allowRateLimit);
        }
      } catch (_) {}
    } else if (res.status === 409 || res.status === 422) {
      businessRejectedCounter.add(1);
    }
  } else {
    const res = http.post(
      `${cfg.engineUrl}/api/v1/enrollments/drop`,
      JSON.stringify({ sectionId: sectionId }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `rb-drop-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "engine_drop" },
      }
    );
    dropTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
    check(res, {
      "drop valid status": (r) =>
        r.status === 202 || r.status === 409 || r.status === 422 || (cfg.allowRateLimit && r.status === 429),
    });
    if (res.status === 202) {
      acceptedCounter.add(1);
    } else if (res.status === 409 || res.status === 422) {
      businessRejectedCounter.add(1);
    }
  }
}
