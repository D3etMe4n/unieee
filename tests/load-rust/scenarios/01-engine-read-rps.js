import http from "k6/http";
import { check } from "k6";
import { Trend } from "k6/metrics";
import { resolveRuntimeConfig } from "../lib/profiles.js";
import {
  authHeaders,
  loadBenchContext,
  pickStudent,
  recordUnexpectedStatus,
} from "../lib/common.js";

const cfg = resolveRuntimeConfig("read");
const ctx = loadBenchContext();

if (cfg.allowRateLimit) {
  http.setResponseCallback(http.expectedStatuses(200, 429));
} else {
  http.setResponseCallback(http.expectedStatuses(200));
}

const hotSeatsTrend = new Trend("unieee_engine_seats_ms");
const hotMeTrend = new Trend("unieee_engine_me_ms");

export const options = Object.assign(
  {
    thresholds: {
      unieee_unexpected_error_rate: ["rate<0.001"],
      unieee_engine_seats_ms: ["p(95)<50", "p(99)<150"],
      unieee_engine_me_ms: ["p(95)<50", "p(99)<150"],
    },
  },
  cfg.targetRps > 0
    ? {
        scenarios: {
          engine_read: {
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
  const student = pickStudent(ctx);
  if (Math.random() < 0.70) {
    const res = http.get(`${cfg.engineUrl}/api/v1/enrollments/seats?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "engine_seats" },
    });
    hotSeatsTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, null, cfg.allowRateLimit);
    check(res, {
      "engine seats ok": (r) => r.status === 200 || (cfg.allowRateLimit && r.status === 429),
    });
  } else {
    const res = http.get(`${cfg.engineUrl}/api/v1/enrollments/me?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "engine_me" },
    });
    hotMeTrend.add(res.timings.duration);
    recordUnexpectedStatus(res, null, cfg.allowRateLimit);
    check(res, {
      "engine me ok": (r) => r.status === 200 || (cfg.allowRateLimit && r.status === 429),
    });
  }
}
