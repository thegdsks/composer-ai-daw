// Change feed limits (D25). Read from the environment like config.ts, no thresholds in logic.

type Env = Record<string, string | undefined>;
const env: Env = (import.meta as { env?: Env }).env ?? {};

function num(key: string, fallback: number): number {
  const raw = env[key];
  const n = raw === undefined || raw === '' ? NaN : Number(raw);
  return Number.isFinite(n) && n >= 0 ? n : fallback;
}

export const feedConfig = {
  /** Largest edit the wasm editor lists change by change; bigger ones are reported as full. */
  maxChanges: num('VITE_MAX_FEED_CHANGES', 5000),
  /** Verify the mirror against a full fetch after this many applied feeds (0 never). */
  verifyEvery: num('VITE_MIRROR_VERIFY_EVERY', 100),
};
