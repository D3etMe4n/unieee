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

http.setResponseCallback(http.expectedStatuses(200, 202, 409, 422));

const cfg = resolveRuntimeConfig();
const ctx = loadBenchContext();

const registerTrend = new Trend("unieee_hot_register_ms");
const dropTrend = new Trend("unieee_hot_drop_ms");
const statusTrend = new Trend("unieee_hot_status_ms");
const acceptedCounter = new Counter("unieee_hot_accepted_total");
const businessRejectedCounter = new Counter("unieee_hot_business_rejected_total");

export const options = {
  insecureSkipTLSVerify: true,
  vus: cfg.vus,
  duration: cfg.duration,
  thresholds: {
    unieee_unexpected_error_rate: ["rate<0.001"],
    unieee_hot_register_ms: ["p(95)<50", "p(99)<150"],
  },
};

export default function () {
  const student = stableStudentForVu(ctx);
  const allTargets = ctx.hotSectionIds.concat(ctx.normalSectionIds.slice(0, 10));
  const useHot = Math.random() < 0.80;
  const pool = useHot ? ctx.hotSectionIds : allTargets;
  const sectionId = pool[Math.floor(Math.random() * pool.length)];

  const roll = Math.random();
  if (roll < 0.82) {
    const res = http.post(
      `${cfg.baseUrl}/api/v1/enrollments/register`,
      JSON.stringify({ sectionId: sectionId, action: "REGISTER" }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `k6-reg-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "hot_register" },
      }
    );
    registerTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422]);
    check(res, {
      "register valid 202 or 409/422": (r) => r.status === 202 || r.status === 409 || r.status === 422,
    });
    if (res.status === 202) {
      acceptedCounter.add(1);
      try {
        const body = JSON.parse(res.body);
        if (body && body.eventId && Math.random() < 0.25) {
          const stRes = http.get(`${cfg.baseUrl}/api/v1/enrollments/status/${body.eventId}`, {
            headers: authHeaders(student.token),
            tags: { endpoint: "hot_status" },
          });
          statusTrend.add(stRes.timings.duration);
          recordUnexpectedStatus(stRes, [404]);
        }
      } catch (_) {}
    } else if (res.status === 409 || res.status === 422) {
      businessRejectedCounter.add(1);
    }
  } else {
    const res = http.post(
      `${cfg.baseUrl}/api/v1/enrollments/drop`,
      JSON.stringify({ sectionId: sectionId }),
      {
        headers: authHeaders(student.token, {
          "Content-Type": "application/json",
          "Idempotency-Key": `k6-drop-${student.code}-${__VU}-${__ITER}`,
        }),
        tags: { endpoint: "hot_drop" },
      }
    );
    dropTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, [409, 422]);
    check(res, {
      "drop valid 202 or 409/422": (r) => r.status === 202 || r.status === 409 || r.status === 422,
    });
    if (res.status === 202) {
      acceptedCounter.add(1);
    } else if (res.status === 409 || res.status === 422) {
      businessRejectedCounter.add(1);
    }
  }
}
