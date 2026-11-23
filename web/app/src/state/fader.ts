// Fader math. The project stores volume in permille of unity gain (1000 is 0 dB); the fader
// shows decibels. Pure, so the mapping is testable.
import { config } from '../config';

/** Decibels for a volume. Zero volume reads as the fader floor. */
export function permilleToDb(permille: number): number {
  if (permille <= 0) return config.faderMinDb;
  return Math.max(config.faderMinDb, 20 * Math.log10(permille / config.volumeUnity));
}

/** Highest fader value in dB, the top of the volume range. */
export function faderMaxDb(): number {
  return permilleToDb(config.volumeMax);
}

/** Volume for a fader value. At or below the floor the track is silent; the top maps to the maximum. */
export function dbToPermille(db: number): number {
  if (db <= config.faderMinDb) return 0;
  const permille = Math.round(config.volumeUnity * 10 ** (db / 20));
  return Math.min(config.volumeMax, permille);
}

export function clampDb(db: number): number {
  return Math.min(faderMaxDb(), Math.max(config.faderMinDb, db));
}

/** Where a fader value sits on the track, 0 at the floor and 1 at the top. */
export function dbFraction(db: number): number {
  const lo = config.faderMinDb;
  return (clampDb(db) - lo) / (faderMaxDb() - lo);
}

export function fractionToDb(fraction: number): number {
  const lo = config.faderMinDb;
  return clampDb(lo + Math.min(1, Math.max(0, fraction)) * (faderMaxDb() - lo));
}

export function dbText(db: number): string {
  if (db <= config.faderMinDb) return 'Silent';
  const r = Math.round(db * 10) / 10;
  return `${r > 0 ? '+' : ''}${r.toFixed(1)} dB`;
}
