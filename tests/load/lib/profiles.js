export const BENCH_PROFILES = {
  smoke: {
    vus: 10,
    duration: "15s",
    students: 20,
    normalSections: 10,
    hotSections: 2,
    hotSectionCap: 5,
    normalSectionCap: 100,
    sseClients: 10,
  },
  laptop: {
    vus: 40,
    duration: "30s",
    students: 100,
    normalSections: 30,
    hotSections: 5,
    hotSectionCap: 15,
    normalSectionCap: 150,
    sseClients: 25,
  },
  workstation: {
    vus: 100,
    duration: "45s",
    students: 250,
    normalSections: 60,
    hotSections: 10,
    hotSectionCap: 25,
    normalSectionCap: 250,
    sseClients: 60,
  },
  stress: {
    vus: 250,
    duration: "60s",
    students: 500,
    normalSections: 120,
    hotSections: 20,
    hotSectionCap: 50,
    normalSectionCap: 500,
    sseClients: 120,
  },
};

export function resolveRuntimeConfig() {
  const profileName = (__ENV.BENCH_PROFILE || "laptop").toLowerCase();
  const preset = BENCH_PROFILES[profileName];
  if (!preset) {
    throw new Error(
      `Unsupported BENCH_PROFILE='${profileName}'. Expected: smoke | laptop | workstation | stress`
    );
  }

  const vus = Number(__ENV.VUS || preset.vus);
  const duration = __ENV.DURATION || preset.duration;
  const targetRps = __ENV.TARGET_RPS ? Number(__ENV.TARGET_RPS) : 0;
  const baseUrl = (__ENV.BASE_URL || "https://127.0.0.1:8443").replace(/\/+$/, "");

  return {
    profileName,
    vus,
    duration,
    targetRps,
    baseUrl,
    preset,
  };
}
