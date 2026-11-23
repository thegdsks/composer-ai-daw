// Track meter estimate. The engine only reports its summed output, so each strip shows what the
// Per track level: the engine's measured peaks when the worker sends them (cw_track_peaks), else
// an estimate of what the project says should be sounding (held notes, velocity, gain, mute and
// solo). The estimate stays for the daemon-only case and older workers.
import { config } from '../config';
import { mixOf, notesOf, type Project, type Tick, type Track } from '../model';

/** False when the track is muted, or when another track is soloed and this one is not. */
export function isAudible(project: Project, track: Track): boolean {
  const mix = mixOf(track);
  if (mix.mute) return false;
  const anySolo = project.tracks.some((t) => mixOf(t).solo);
  return !anySolo || mix.solo;
}

/** Loudest held note on the track at `tick`, 0 to 1 before the volume is applied. */
export function heldLevel(track: Track, tick: Tick): number {
  let best = 0;
  for (const region of Object.values(track.regions)) {
    if (tick < region.start || tick >= region.start + region.length) continue;
    const local = tick - region.start;
    for (const n of notesOf(region)) {
      if (n.start <= local && local < n.start + n.duration) best = Math.max(best, n.velocity / 127);
    }
  }
  return best;
}

/** Linear level in 0..1 for the track while the transport plays; 0 when silent. */
export function trackLevel(project: Project, track: Track, tick: Tick, playing: boolean): number {
  if (!playing || !isAudible(project, track)) return 0;
  const gain = mixOf(track).volume / config.volumeUnity;
  return Math.min(1, heldLevel(track, tick) * gain);
}

/** Lit segments out of config.meterSegments for a linear level, on a dB scale. */
export function litSegments(level: number): number {
  if (level <= 0) return 0;
  const db = 20 * Math.log10(level);
  const fraction = (db - config.meterFloorDb) / -config.meterFloorDb;
  return Math.max(0, Math.min(config.meterSegments, Math.ceil(fraction * config.meterSegments)));
}

/**
 * Level for one track: the measured peak at the track's index when the report covers every track
 * of the project, else the estimate (which needs the transport to play).
 */
export function trackMeterLevel(
  project: Project,
  track: Track,
  tick: Tick,
  playing: boolean,
  peaks: Float32Array,
): number {
  const index = project.tracks.indexOf(track);
  if (index >= 0 && peaks.length === project.tracks.length) return Math.min(1, peaks[index] ?? 0);
  return trackLevel(project, track, tick, playing);
}
