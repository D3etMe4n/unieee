import { SharedArray } from "k6/data";
import { Rate } from "k6/metrics";

export const unexpectedErrorRate = new Rate("unieee_unexpected_error_rate");
export const nginxCacheHitRate = new Rate("unieee_nginx_cache_hit_rate");

export function loadBenchContext() {
  return new SharedArray("unieee-bench-context", function () {
    const path = __ENV.BENCH_CONTEXT_FILE || "/work/bench-context.json";
    return [JSON.parse(open(path))];
  })[0];
}

export function authHeaders(token, extra) {
  const headers = {
    Authorization: `Bearer ${token}`,
  };
  if (extra) {
    for (const k of Object.keys(extra)) {
      headers[k] = extra[k];
    }
  }
  return headers;
}

export function pickStudent(ctx) {
  const idx = (__VU - 1 + __ITER) % ctx.students.length;
  return ctx.students[idx];
}

export function stableStudentForVu(ctx) {
  const idx = (__VU - 1) % ctx.students.length;
  return ctx.students[idx];
}

export function recordUnexpectedStatus(res, allowedExtraStatuses) {
  const s = res.status;
  if (s === 0 || s >= 500) {
    unexpectedErrorRate.add(true);
    return false;
  }
  if (s >= 200 && s < 300) {
    unexpectedErrorRate.add(false);
    return true;
  }
  if (allowedExtraStatuses && allowedExtraStatuses.indexOf(s) !== -1) {
    unexpectedErrorRate.add(false);
    return true;
  }
  unexpectedErrorRate.add(true);
  return false;
}

export function recordCacheHeader(res) {
  const cacheHeader =
    res.headers["X-Cache-Status"] ||
    res.headers["x-cache-status"] ||
    "";
  nginxCacheHitRate.add(cacheHeader.toUpperCase() === "HIT");
}
