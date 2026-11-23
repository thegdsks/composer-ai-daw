// Client mirror of the project, kept in step by the change feed (D25). Every feed names the
// state hash it applies to; a feed is applied only when that hash equals the mirror's own
// hash. Anything else (a gap, a feed marked full, a change that does not fit) falls back to
// one full state fetch. A delta is never applied onto a base that is not proven equal.
import { config } from '../config';
import type { Command, Id, Note, Project, Region, Track } from '../model';
import { feedConfig } from './feedConfig';
import type {
  Backend,
  Change,
  ChangeFeed,
  FieldKey,
  FieldValue,
  Reply,
  StateChange,
  StateSnapshot,
  Summary,
} from './types';

/** Counters for composerDebug and the risk measure K5 (fallbacks per session). */
export interface MirrorStats {
  fullFetches: number;
  /** Full fetches caused by a feed that could not be used. */
  fallbacks: number;
  feedsApplied: number;
  changesApplied: number;
  /** Feeds ignored because the mirror already had their result. */
  duplicates: number;
  /** Sampled drift checks run (verifyEvery) and how many found the mirror different from the backend. */
  audits: number;
  auditFailures: number;
}

export interface MirrorOptions {
  /** Verify against a full fetch after this many applied feeds; 0 never. Default from env. */
  verifyEvery?: number;
  /** Called after every update with the new project (a new object when anything changed). */
  onChange?: (project: Project) => void;
  /** Called when a sampled check found the mirror different from the backend and replaced it. */
  onDrift?: (state: Mirrored) => void;
}

/** The mirror at a known state hash, with the history depths of that state. */
export interface Mirrored {
  project: Project;
  hash: string;
  undoDepth: number;
  redoDepth: number;
}

/** A feed that does not fit the mirror; the caller falls back to a full fetch. */
class FeedMismatch extends Error {}

type TempoMapWithKeys = Project['tempo_map'];
type MixValue = Extract<FieldValue, { type: 'mix' }>['value'];

function isDefaultMix(m: MixValue): boolean {
  return m.volume === config.volumeUnity && m.pan === 0 && !m.mute && !m.solo;
}

function mismatch(what: string): never {
  throw new FeedMismatch(what);
}

/**
 * One feed applied to a draft of the project. Objects are copied the first time a feed touches
 * them (`own`), so unchanged tracks, regions and note maps stay shared with the old project.
 */
class Draft {
  readonly project: Project;
  private readonly owned = new WeakSet<object>();
  private readonly regionTrack: Map<Id, Id>;

  constructor(base: Project, regionTrack: Map<Id, Id>) {
    this.project = { ...base, tracks: base.tracks.slice() };
    this.owned.add(this.project);
    this.owned.add(this.project.tracks);
    this.regionTrack = regionTrack;
  }

  private own<T extends object>(o: T, copy: (x: T) => T): T {
    if (this.owned.has(o)) return o;
    const c = copy(o);
    this.owned.add(c);
    return c;
  }

  private trackIndex(id: Id): number {
    const i = this.project.tracks.findIndex((t) => t.id === id);
    if (i < 0) mismatch('track');
    return i;
  }

  private ownTrack(id: Id): Track {
    const i = this.trackIndex(id);
    const t = this.project.tracks[i];
    if (!t) mismatch('track');
    if (this.owned.has(t)) return t;
    const c = this.own(t, (x) => ({ ...x, regions: { ...x.regions } }));
    this.owned.add(c.regions);
    this.project.tracks[i] = c;
    return c;
  }

  /** Regions already copied by this feed: a bulk feed touches the same clip thousands of times. */
  private readonly ownedRegions = new Map<Id, Region>();

  private ownRegion(regionId: Id): Region & { content: { midi: { notes: Record<Id, Note> } } } {
    const fast = this.ownedRegions.get(regionId);
    if (fast) return fast as never;
    const trackId = this.regionTrack.get(regionId);
    if (trackId === undefined) mismatch('region');
    const t = this.ownTrack(trackId);
    const r = t.regions[regionId];
    if (!r) mismatch('region');
    if (!('midi' in r.content)) mismatch('midi region');
    if (this.owned.has(r)) return r as never;
    const c: Region = { ...r, content: { midi: { notes: { ...r.content.midi.notes } } } };
    this.owned.add(c);
    this.owned.add((c.content as { midi: object }).midi);
    t.regions[regionId] = c;
    this.ownedRegions.set(regionId, c);
    return c as never;
  }

  private indexRegions(t: Track, on: boolean): void {
    for (const id of Object.keys(t.regions)) {
      this.ownedRegions.delete(id);
      if (on) this.regionTrack.set(id, t.id);
      else this.regionTrack.delete(id);
    }
  }

  private ownTempo(): TempoMapWithKeys {
    const p = this.project;
    if (this.owned.has(p.tempo_map)) return p.tempo_map;
    const tm = this.own(p.tempo_map as TempoMapWithKeys, (x) => ({
      ...x,
      tempos: { ...x.tempos },
      meters: { ...x.meters },
      ...(x.keys ? { keys: { ...x.keys } } : {}),
    }));
    p.tempo_map = tm;
    return tm;
  }

  apply(c: Change): void {
    const p = this.project;
    switch (c.op) {
      case 'new_track': {
        if (c.index > p.tracks.length) mismatch('track index');
        p.tracks.splice(c.index, 0, c.track);
        this.indexRegions(c.track, true);
        return;
      }
      case 'delete_track': {
        const at = p.tracks[c.index];
        if (at?.id !== c.track.id) mismatch('track position');
        this.indexRegions(at, false);
        p.tracks.splice(c.index, 1);
        return;
      }
      case 'new_region': {
        const t = this.ownTrack(c.track);
        t.regions[c.region.id] = c.region;
        this.ownedRegions.delete(c.region.id);
        this.regionTrack.set(c.region.id, c.track);
        return;
      }
      case 'delete_region': {
        const t = this.ownTrack(c.track);
        if (!t.regions[c.region.id]) mismatch('region');
        delete t.regions[c.region.id];
        this.ownedRegions.delete(c.region.id);
        this.regionTrack.delete(c.region.id);
        return;
      }
      case 'new_note': {
        this.ownRegion(c.region).content.midi.notes[c.note.id] = c.note;
        return;
      }
      case 'delete_note': {
        const notes = this.ownRegion(c.region).content.midi.notes;
        if (!notes[c.note.id]) mismatch('note');
        delete notes[c.note.id];
        return;
      }
      case 'set':
        this.set(c.key, c.old, c.new);
        return;
      case 'set_tempo': {
        const tm = this.ownTempo();
        if ((tm.tempos[String(c.tick)] ?? null) !== c.old) mismatch('tempo');
        if (c.new === null) delete tm.tempos[String(c.tick)];
        else tm.tempos[String(c.tick)] = c.new;
        return;
      }
      case 'set_meter': {
        const tm = this.ownTempo();
        const cur = tm.meters[String(c.tick)];
        if ((cur?.numerator ?? null) !== (c.old?.numerator ?? null)) mismatch('meter');
        if (c.new === null) delete tm.meters[String(c.tick)];
        else tm.meters[String(c.tick)] = c.new;
        return;
      }
      case 'set_key': {
        const tm = this.ownTempo();
        const cur = tm.keys?.[String(c.tick)];
        if ((cur?.root ?? null) !== (c.old?.root ?? null)) mismatch('key');
        const keys = { ...(tm.keys ?? {}) };
        if (c.new === null) delete keys[String(c.tick)];
        else keys[String(c.tick)] = c.new;
        // The core leaves the field out while empty, so the mirror does too.
        if (Object.keys(keys).length === 0) delete tm.keys;
        else tm.keys = keys;
        return;
      }
      case 'set_marker': {
        const cur = p.markers?.[String(c.tick)] ?? null;
        if (cur !== c.old) mismatch('marker');
        const markers = { ...(p.markers ?? {}) };
        if (c.new === null) delete markers[String(c.tick)];
        else markers[String(c.tick)] = c.new;
        // The core leaves the field out while empty, so the mirror does too.
        if (Object.keys(markers).length === 0) delete p.markers;
        else p.markers = markers;
        return;
      }
      case 'insert_effect':
      case 'remove_effect':
      case 'move_effect':
      case 'set_effect_bypass':
      case 'set_effect_param':
        // Chains are small and rare; one full fetch is simpler than a second copy of the model.
        mismatch('effect chain');
    }
  }

  private set(key: FieldKey, old: FieldValue, next: FieldValue): void {
    const same = (a: unknown, b: unknown) => a === b;
    const p = this.project;
    switch (key.field) {
      case 'project_name':
        if (!same(p.name, old.value)) mismatch('project name');
        p.name = next.value as string;
        return;
      case 'track_name': {
        const t = this.ownTrack(key.track);
        if (!same(t.name, old.value)) mismatch('track name');
        t.name = next.value as string;
        return;
      }
      case 'track_instrument': {
        const t = this.ownTrack(key.track);
        if (!same(t.instrument, old.value)) mismatch('instrument');
        t.instrument = next.value as Id | null;
        return;
      }
      case 'track_mix': {
        const t = this.ownTrack(key.track);
        const m = next.value as MixValue;
        if (isDefaultMix(m)) delete t.mix;
        else t.mix = { ...m };
        return;
      }
      case 'track_sound': {
        const t = this.ownTrack(key.track);
        if (!same(t.sound ?? null, old.value)) mismatch('sound');
        if (next.value === null) delete t.sound;
        else t.sound = next.value as string;
        return;
      }
      case 'region_start':
      case 'region_length':
      case 'region_loop': {
        const trackId = this.regionTrack.get(key.region);
        if (trackId === undefined) mismatch('region');
        const t = this.ownTrack(trackId);
        const r = t.regions[key.region];
        if (!r) mismatch('region');
        this.ownedRegions.delete(key.region);
        if (key.field === 'region_loop') {
          if (!same(r.loop_length ?? 0, old.value)) mismatch('region loop');
          const copy: Region = { ...r };
          if (next.value === 0) delete copy.loop_length;
          else copy.loop_length = next.value as number;
          t.regions[key.region] = copy;
          return;
        }
        const field = key.field === 'region_start' ? 'start' : 'length';
        if (!same(r[field], old.value)) mismatch('region field');
        t.regions[key.region] = { ...r, [field]: next.value as number };
        return;
      }
      case 'note_pitch':
      case 'note_velocity':
      case 'note_start':
      case 'note_duration': {
        const notes = this.ownRegion(key.region).content.midi.notes;
        const n = notes[key.note];
        if (!n) mismatch('note');
        const field = { note_pitch: 'pitch', note_velocity: 'velocity', note_start: 'start', note_duration: 'duration' }[
          key.field
        ] as 'pitch' | 'velocity' | 'start' | 'duration';
        if (!same(n[field], old.value)) mismatch('note field');
        notes[key.note] = { ...n, [field]: next.value as number };
        return;
      }
    }
  }
}

function indexRegions(p: Project): Map<Id, Id> {
  const m = new Map<Id, Id>();
  for (const t of p.tracks) for (const id of Object.keys(t.regions)) m.set(id, t.id);
  return m;
}

export class ProjectMirror {
  private snapshot: Project | null = null;
  private regionTrack = new Map<Id, Id>();
  private current: string | null = null;
  private depths = { undo: 0, redo: 0 };
  private queue: Promise<unknown> = Promise.resolve();
  private unsubscribe: (() => void) | null = null;
  private sinceVerify = 0;
  readonly stats: MirrorStats = { fullFetches: 0, fallbacks: 0, feedsApplied: 0, changesApplied: 0, duplicates: 0, audits: 0, auditFailures: 0 };

  constructor(
    private readonly backend: Backend,
    private readonly options: MirrorOptions = {},
  ) {}

  /** The mirrored project; a new object after every update that changed it. */
  get project(): Project | null {
    return this.snapshot;
  }

  /** State hash the mirror stands at. */
  get hash(): string | null {
    return this.current;
  }

  get undoDepth(): number {
    return this.depths.undo;
  }

  get redoDepth(): number {
    return this.depths.redo;
  }

  /** Fetches the full state, then follows state.changed pushes of other clients. */
  async open(): Promise<Reply<StateSnapshot>> {
    const r = await this.enqueue(() => this.fetchFull());
    if (r.ok && !this.unsubscribe) this.unsubscribe = this.backend.subscribe((c) => this.onPush(c));
    return r;
  }

  /**
   * Starts from a project the caller already fetched (the Store does its own opening fetch), so
   * no second full fetch is needed. The mirror does not subscribe to pushes then: the caller
   * hands each push and edit summary to `sync`.
   */
  seed(project: Project, hash: string, undoDepth: number, redoDepth: number): void {
    this.snapshot = project;
    this.regionTrack = indexRegions(project);
    this.current = hash;
    this.depths = { undo: undoDepth, redo: redoDepth };
    this.sinceVerify = 0;
  }

  /**
   * Brings the mirror to a hash an edit reply or a push announced, using that edit's feed.
   * Resolves with the mirrored project once it stands at `hash`; null when it does not (the
   * backend moved on, or the full fetch failed) and the caller should do a full refresh.
   */
  sync(hash: string, feed: ChangeFeed | undefined, depths?: { undo: number; redo: number }): Promise<Mirrored | null> {
    return this.enqueue(async () => {
      if (depths) this.depths = depths;
      await this.follow(hash, feed);
      if (!this.snapshot || this.current !== hash) return null;
      return { project: this.snapshot, hash, undoDepth: this.depths.undo, redoDepth: this.depths.redo };
    });
  }

  dispose(): void {
    this.unsubscribe?.();
    this.unsubscribe = null;
  }

  apply(command: Command): Promise<Reply<Summary>> {
    return this.edit(() => this.backend.apply(command));
  }

  group(label: string, commands: Command[]): Promise<Reply<Summary>> {
    return this.edit(() => this.backend.group(label, commands));
  }

  undo(): Promise<Reply<Summary>> {
    return this.edit(() => this.backend.undo());
  }

  redo(): Promise<Reply<Summary>> {
    return this.edit(() => this.backend.redo());
  }

  /** Sampled drift check: a full fetch must equal the mirror in hash and content, else the fetch replaces it. */
  private async audit(): Promise<void> {
    const s = await this.backend.state();
    if (!s.ok) return;
    this.stats.audits++;
    const same = s.value.stateHash === this.current && deepEqual(s.value.project, this.snapshot);
    if (same) return;
    this.stats.auditFailures++;
    this.stats.fullFetches++;
    this.stats.fallbacks++;
    this.snapshot = s.value.project;
    this.regionTrack = indexRegions(s.value.project);
    this.current = s.value.stateHash;
    this.depths = { undo: s.value.undoDepth, redo: s.value.redoDepth };
    this.options.onChange?.(s.value.project);
    this.options.onDrift?.({ project: s.value.project, hash: s.value.stateHash, undoDepth: s.value.undoDepth, redoDepth: s.value.redoDepth });
  }

  /** Full fetch compared with the mirror: the project must be deeply equal. For tests and debug. */
  async verify(): Promise<boolean> {
    return this.enqueue(async () => {
      const s = await this.backend.state();
      if (!s.ok || s.value.stateHash !== this.current) return false;
      return JSON.stringify(sortKeys(s.value.project)) === JSON.stringify(sortKeys(this.snapshot));
    });
  }

  private edit(run: () => Promise<Reply<Summary>>): Promise<Reply<Summary>> {
    return this.enqueue(async () => {
      const r = await run();
      if (!r.ok) return r;
      this.depths = { undo: r.value.undoDepth, redo: r.value.redoDepth };
      await this.follow(r.value.stateHash, r.value.feed);
      return r;
    });
  }

  private onPush(c: StateChange): void {
    void this.enqueue(() => this.follow(c.hash, c.feed));
  }

  private enqueue<T>(job: () => Promise<T>): Promise<T> {
    const next = this.queue.then(job, job);
    this.queue = next.catch(() => undefined);
    return next;
  }

  /** Brings the mirror to `hash`: skip, apply the feed, or fetch everything. */
  private async follow(hash: string, feed: ChangeFeed | undefined): Promise<void> {
    if (this.snapshot && this.current === hash) {
      this.stats.duplicates++;
      return;
    }
    const usable = feed?.changes && feed.baseHash === this.current && feed.hash === hash;
    if (this.snapshot && feed?.changes && usable && this.applyFeed(feed.changes, feed.hash)) {
      this.sinceVerify++;
      const every = this.options.verifyEvery ?? feedConfig.verifyEvery;
      if (every > 0 && this.sinceVerify >= every) {
        this.sinceVerify = 0;
        // Off the edit's own path: the edit resolves first, the check runs next in the queue.
        void this.enqueue(() => this.audit());
      }
      return;
    }
    // A feed marked full needs the whole state. Otherwise the client may have missed pushes:
    // ask for the gap before giving up.
    const full = feed !== undefined && feed.changes === null;
    if (this.snapshot && this.current !== null && this.backend.changesSince && !full) {
      const gap = await this.backend.changesSince(this.current);
      if (gap.ok && gap.value.changes && gap.value.baseHash === this.current && gap.value.hash === hash) {
        if (this.applyFeed(gap.value.changes, gap.value.hash)) return;
      }
    }
    await this.fetchFull(this.snapshot !== null);
  }

  /** Applies a list onto a draft and commits it only when every change fits. */
  private applyFeed(changes: Change[], hash: string): boolean {
    if (!this.snapshot) return false;
    const regionTrack = new Map(this.regionTrack);
    const draft = new Draft(this.snapshot, regionTrack);
    try {
      for (const c of changes) draft.apply(c);
    } catch (e) {
      if (e instanceof FeedMismatch) return false;
      throw e;
    }
    this.snapshot = draft.project;
    this.regionTrack = regionTrack;
    this.current = hash;
    this.stats.feedsApplied++;
    this.stats.changesApplied += changes.length;
    this.options.onChange?.(draft.project);
    return true;
  }

  private async fetchFull(fallback = false): Promise<Reply<StateSnapshot>> {
    const s = await this.backend.state();
    if (!s.ok) return s;
    this.stats.fullFetches++;
    if (fallback) this.stats.fallbacks++;
    this.snapshot = s.value.project;
    this.regionTrack = indexRegions(s.value.project);
    this.current = s.value.stateHash;
    this.depths = { undo: s.value.undoDepth, redo: s.value.redoDepth };
    this.options.onChange?.(s.value.project);
    return s;
  }
}

/** Object keys in sorted order, so two projects compare equal whatever their insertion order. */
function sortKeys(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.entries(v as Record<string, unknown>)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([k, x]) => [k, sortKeys(x)]),
    );
  }
  return v;
}

/** Structural equality, key order ignored; far cheaper than sorting and serializing a large project. */
export function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (!deepEqual(a[i], b[i])) return false;
    return true;
  }
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  let count = 0;
  for (const k in x) {
    count++;
    if (!(k in y) || !deepEqual(x[k], y[k])) return false;
  }
  return count === Object.keys(y).length;
}
