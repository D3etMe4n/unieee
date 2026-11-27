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

const publicCoursesTrend = new Trend("unieee_warm_public_courses_ms");
const publicSectionsTrend = new Trend("unieee_warm_public_sections_ms");
const authSectionsTrend = new Trend("unieee_warm_sections_ms");
const timetableTrend = new Trend("unieee_warm_timetable_ms");
const policyTrend = new Trend("unieee_warm_policy_ms");

export const options = Object.assign(
  {
    insecureSkipTLSVerify: true,
    thresholds: {
      unieee_unexpected_error_rate: ["rate<0.001"],
    },
  },
  cfg.targetRps > 0
    ? {
        scenarios: {
          warm_read: {
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
  const roll = Math.random();

  if (roll < 0.30) {
    const res = http.get(`${cfg.baseUrl}/api/v1/public/sections?termId=${ctx.termId}`, {
      tags: { endpoint: "public_sections" },
    });
    publicSectionsTrend.add(res.timings.duration);
    recordCacheHeader(res);
    recordUnexpectedStatus(res);
    check(res, { "public sections 200": (r) => r.status === 200 });
  } else if (roll < 0.45) {
    const res = http.get(`${cfg.baseUrl}/api/v1/public/courses`, {
      tags: { endpoint: "public_courses" },
    });
    publicCoursesTrend.add(res.timings.duration);
    recordCacheHeader(res);
    recordUnexpectedStatus(res);
    check(res, { "public courses 200": (r) => r.status === 200 });
  } else if (roll < 0.70) {
    const res = http.get(`${cfg.baseUrl}/api/v1/sections?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "auth_sections" },
    });
    authSectionsTrend.add(res.timings.duration);
    recordUnexpectedStatus(res);
    check(res, { "sections 200": (r) => r.status === 200 });
  } else if (roll < 0.88) {
    const res = http.get(`${cfg.baseUrl}/api/v1/students/me/timetable?termId=${ctx.termId}`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "student_timetable" },
    });
    timetableTrend.add(res.timings.duration);
    recordUnexpectedStatus(res);
    check(res, { "timetable 200": (r) => r.status === 200 });
  } else {
    const res = http.get(`${cfg.baseUrl}/api/v1/policies/terms/${ctx.termId}/active`, {
      headers: authHeaders(student.token),
      tags: { endpoint: "active_policy" },
    });
    policyTrend.add(res.timings.duration);
    recordUnexpectedStatus(res);
    check(res, { "active policy 200": (r) => r.status === 200 });
  }
}
