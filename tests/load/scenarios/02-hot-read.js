import http from "k6/http";
import { check } from "k6";
import { Trend } from "k6/metrics";
import { resolveRuntimeConfig } from "../lib/profiles.js";
import {
  authHeaders,
  loadBenchContext,
  pickStudent,
  recordCacheHeader,
  recordUnexpectedStatus,
} from "../lib/common.js";

const cfg = resolveRuntimeConfig();
const ctx = loadBenchContext();

const hotSeatsTrend = new Trend("unieee_hot_seats_ms");
const hotMeTrend = new Trend("unieee_hot_me_ms");

export const options = Object.assign(
  {
    insecureSkipTLSVerify: true,
    thresholds: {
      unieee_unexpected_error_rate: ["rate<0.001"],
      unieee_hot_seats_ms: ["p(95)<50", "p(99)<150"],
      unieee_hot_me_ms: ["p(95)<50", "p(99)<150"],
    },
  },
  cfg.targetRps > 0
    ? {
        scenarios: {
          hot_read: {
            executor: "constant-arrival-rate",
            rate: cfg.targetRps,
            timeUnit: "1s",
            duration: cfg.duration,
            preAllocatedVUs: cfg.vus,
            maxVUs: Math.max(cfg.vus * 2, 20),
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
    const res = http.get(`${cfg.baseUrl}/api/v1/enrollments/seats?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "hot_seats" },
    });
    hotSeatsTrend.add(res.timings.duration);
    recordCacheHeader(res);
    recordUnexpectedStatus(res);
    check(res, { "hot seats 200": (r) => r.status === 200 });
  } else {
    const res = http.get(`${cfg.baseUrl}/api/v1/enrollments/me?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "hot_me" },
    });
    hotMeTrend.add(res.timings.duration);
    recordUnexpectedStatus(res);
    check(res, { "hot me 200": (r) => r.status === 200 });
  }
}
