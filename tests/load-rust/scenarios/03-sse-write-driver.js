import http from "k6/http";
import { check, sleep } from "k6";
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
  http.setResponseCallback(http.expectedStatuses(202, 409, 422, 429));
} else {
  http.setResponseCallback(http.expectedStatuses(202, 409, 422));
}

const registerTrend = new Trend("unieee_sse_driver_register_ms");
const dropTrend = new Trend("unieee_sse_driver_drop_ms");
const acceptedCounter = new Counter("unieee_sse_driver_accepted_total");

// Per-VU held section so seats continuously churn via register -> drop without saturating
let heldSectionId = null;

export const options = {
  vus: cfg.vus,
  duration: cfg.duration,
  thresholds: {
    unieee_unexpected_error_rate: ["rate<0.001"],
  },
};

export default function () {
  const student = stableStudentForVu(ctx);
  const churnPool = ctx.churnSectionIds && ctx.churnSectionIds.length > 0
    ? ctx.churnSectionIds
    : ctx.normalSectionIds;

  if (heldSectionId === null) {
    const sectionId = churnPool[(__VU - 1 + __ITER) % churnPool.length];
    const res = http.post(
      `${cfg.engineUrl}/api/v1/enrollments/register`,
      JSON.stringify({ sectionId: sectionId, action: "REGISTER" }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `sse-reg-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "sse_driver_register" },
      }
    );
    registerTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
    check(res, {
      "sse driver register valid": (r) =>
        r.status === 202 || r.status === 409 || r.status === 422 || (cfg.allowRateLimit && r.status === 429),
    });
    if (res.status === 202) {
      heldSectionId = sectionId;
      acceptedCounter.add(1);
    }
  } else {
    const sectionId = heldSectionId;
    const res = http.post(
      `${cfg.engineUrl}/api/v1/enrollments/drop`,
      JSON.stringify({ sectionId: sectionId }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `sse-drop-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "sse_driver_drop" },
      }
    );
    dropTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422], cfg.allowRateLimit);
    check(res, {
      "sse driver drop valid": (r) =>
        r.status === 202 || r.status === 409 || r.status === 422 || (cfg.allowRateLimit && r.status === 429),
    });
    if (res.status === 202) {
      heldSectionId = null;
      acceptedCounter.add(1);
    } else if (res.status === 409 || res.status === 422) {
      heldSectionId = null;
    }
  }
  sleep(0.05);
}
