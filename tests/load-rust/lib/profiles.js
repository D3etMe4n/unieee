export const RUST_BENCH_PROFILES = {
  smoke: {
    readVus: 20,
    writeVus: 20,
    mixedVus: 20,
    duration: "15s",
    students: 200,
    hotSections: 3,
    hotCap: 10,
    normalSections: 16,
    normalCap: 100,
    churnSections: 8,
    sseConnections: 100,
    sseHoldSec: 15,
    sseRampRate: 100,
  },
  laptop: {
    readVus: 60,
    writeVus: 60,
    mixedVus: 60,
    duration: "30s",
    students: 1000,
    hotSections: 5,
    hotCap: 25,
    normalSections: 40,
    normalCap: 250,
    churnSections: 12,
    sseConnections: 500,
    sseHoldSec: 25,
    sseRampRate: 250,
  },
  workstation: {
    readVus: 150,
    writeVus: 150,
    mixedVus: 150,
    duration: "45s",
    students: 2500,
    hotSections: 10,
    hotCap: 50,
    normalSections: 80,
    normalCap: 400,
    churnSections: 20,
    sseConnections: 2000,
    sseHoldSec: 35,
    sseRampRate: 500,
  },
  stress: {
    readVus: 300,
    writeVus: 300,
    mixedVus: 300,
    duration: "60s",
    students: 5000,
    hotSections: 20,
    hotCap: 100,
    normalSections: 120,
    normalCap: 500,
    churnSections: 28,
    sseConnections: 5000,
    sseHoldSec: 45,
    sseRampRate: 800,
  },
};

export function resolveRuntimeConfig(mode) {
  const profileName = (__ENV.BENCH_PROFILE || "laptop").toLowerCase();
  const preset = RUST_BENCH_PROFILES[profileName];
  if (!preset) {
    throw new Error(
      `Unsupported BENCH_PROFILE='${profileName}'. Expected: smoke | laptop | workstation | stress`
    );
  }

  let defaultVus = preset.mixedVus;
  if (mode === "read") {
    defaultVus = preset.readVus;
  } else if (mode === "write") {
    defaultVus = preset.writeVus;
  }

  const vus = Number(__ENV.VUS || defaultVus);
  const duration = __ENV.DURATION || preset.duration;
  const targetRps = __ENV.TARGET_RPS ? Number(__ENV.TARGET_RPS) : 0;
  const engineUrl = (__ENV.ENGINE_URL || "http://enrollment-engine:8081").replace(/\/+$/, "");
  const allowRateLimit = (__ENV.ALLOW_RATE_LIMIT || "false").toLowerCase() === "true";

  return {
    profileName,
    vus,
    duration,
    targetRps,
    engineUrl,
    allowRateLimit,
    preset,
  };
}
