// Client state: the last project the backend reported plus selection. All edits go through
// Backend commands; the store never changes the project itself.
import type { Backend, BackendError, FileResult, HistoryPage, Reply, Summary, TransportPatch, TransportPrefs } from '../backend/types';
import { plainError } from './errors';
import type { Command, Id, Note, Project, Region, Track } from '../model';
import { recordCommand, recordEdit } from '../debug';
import { notesOf } from '../model';
import { config } from '../config';
import { ProjectMirror, type MirrorOptions } from '../backend/mirror';

export interface Snapshot {
  /** Bumps on every change, cheap equality for canvases. */
  version: number;
  project: Project;
  stateHash: string;
  undoDepth: number;
  redoDepth: number;
  selectedTrack: Id | null;
  /** Primary clip: the one the Notes editor shows. */
  selectedRegion: Id | null;
  /** Every selected clip, the primary included; Shift click and Shift arrows grow it. */
  selectedRegions: ReadonlySet<Id>;
  selectedNotes: ReadonlySet<Id>;
  /** Loop, metronome, count in and follow: session state the backend keeps (not undoable). */
  transport: TransportPrefs;
  error: string | null;
}

export const DEFAULT_TRANSPORT: TransportPrefs = {
  loop: { on: false, start: 0, end: 0 },
  metronome: { on: false },
  countInBars: 0,
  follow: true,
};

const EMPTY_PROJECT: Project = {
  id: 'ffffffffffffffffffffffffffffffff',
  name: 'untitled',
  tempo_map: { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } },
  tracks: [],
};

const sortedCache = new WeakMap<Region, Note[]>();

/** Notes of a region ordered by start (cached per region object). */
export function sortedNotes(region: Region): Note[] {
  let notes = sortedCache.get(region);
  if (!notes) {
    notes = notesOf(region).sort((a, b) => a.start - b.start || a.pitch - b.pitch);
    sortedCache.set(region, notes);
  }
  return notes;
}

export function findRegion(project: Project, id: Id | null): { track: Track; region: Region } | null {
  if (!id) return null;
  for (const track of project.tracks) {
    const region = track.regions[id];
    if (region) return { track, region };
  }
  return null;
}

export function projectEnd(project: Project): number {
  let end = 0;
  for (const t of project.tracks) for (const r of Object.values(t.regions)) end = Math.max(end, r.start + r.length);
  return end;
}

function countNotes(project: Project): number {
  let n = 0;
  for (const t of project.tracks) for (const r of Object.values(t.regions)) if ('midi' in r.content) n += Object.keys(r.content.midi.notes).length;
  return n;
}

export class Store {
  private snap: Snapshot;
  private readonly listeners = new Set<() => void>();
  private refreshing: Promise<void> | null = null;
  private refreshAgain = false;
  private readonly unsubscribe: () => void;
  private readonly announcers = new Set<(text: string) => void>();
  /** Step labels by history depth, so undo and redo can name the step they moved over. */
  private readonly labels: string[] = [];
  private readonly fullRefreshMax: number;

  /** Follows the change feed instead of refetching, once the project is bigger than the env limit. */
  private mirror: ProjectMirror | null = null;
  private readonly mirrorOptions: MirrorOptions;

  constructor(
    readonly backend: Backend,
    options: { fullRefreshMaxNotes?: number; verifyEvery?: number } = {},
  ) {
    this.fullRefreshMax = options.fullRefreshMaxNotes ?? config.fullRefreshMaxNotes;
    this.mirrorOptions = {
      ...(options.verifyEvery !== undefined ? { verifyEvery: options.verifyEvery } : {}),
      onDrift: (m) => this.adopt(m.project, m.hash, m.undoDepth, m.redoDepth),
    };
    this.snap = {
      version: 0,
      project: EMPTY_PROJECT,
      stateHash: '',
      undoDepth: 0,
      redoDepth: 0,
      selectedTrack: null,
      selectedRegion: null,
      selectedRegions: new Set(),
      selectedNotes: new Set(),
      transport: DEFAULT_TRANSPORT,
      error: null,
    };
    this.unsubscribe = backend.subscribe((c) => {
      if (c.hash === this.snap.stateHash) return;
      if (this.mirror) void this.follow(c.hash, c.feed);
      else void this.refresh();
    });
  }

  dispose(): void {
    this.unsubscribe();
    this.backend.dispose();
  }

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  getSnapshot = (): Snapshot => this.snap;

  /** Plain words for each committed step; the live region in the UI listens here. */
  onAnnounce = (l: (text: string) => void): (() => void) => {
    this.announcers.add(l);
    return () => this.announcers.delete(l);
  };

  private announce(text: string): void {
    for (const l of this.announcers) l(text);
  }

  private commit(r: Summary, fallback: string): void {
    if (r.changed === false) return;
    const label = plainLabel(r.undoLabel?.trim() || fallback);
    this.labels.length = Math.max(0, r.undoDepth - 1);
    this.labels[r.undoDepth - 1] = label;
    this.announce(label);
  }

  /** Label of the history step at a depth (1 is the oldest); used for undo tooltips. */
  stepName(depth: number): string {
    return this.labels[depth - 1] ?? 'the last step';
  }

  private set(patch: Partial<Snapshot>): void {
    this.snap = { ...this.snap, ...patch, version: this.snap.version + 1 };
    for (const l of this.listeners) l();
  }

  /** Fetches the project; overlapping calls collapse into one follow up fetch. */
  refresh(): Promise<void> {
    if (this.refreshing) {
      this.refreshAgain = true;
      return this.refreshing;
    }
    this.refreshing = (async () => {
      do {
        this.refreshAgain = false;
        const r = await this.backend.state();
        if (r.ok) {
          this.adopt(r.value.project, r.value.stateHash, r.value.undoDepth, r.value.redoDepth);
          this.reseedMirror(r.value.project, r.value.stateHash, r.value.undoDepth, r.value.redoDepth);
        } else this.set({ error: r.error.message });
        await this.refreshTransport();
      } while (this.refreshAgain);
    })().finally(() => {
      this.refreshing = null;
    });
    return this.refreshing;
  }

  /** Reads the session prefs; a backend without them keeps the last known values. */
  async refreshTransport(): Promise<void> {
    const r = await this.backend.transportGet();
    if (!r.ok) return;
    const { loop, metronome, countInBars, follow } = r.value;
    const cur = this.snap.transport;
    const same =
      cur.loop.on === loop.on && cur.loop.start === loop.start && cur.loop.end === loop.end &&
      cur.metronome.on === metronome.on && cur.countInBars === countInBars && cur.follow === follow;
    if (!same) this.set({ transport: { loop: { ...loop }, metronome: { ...metronome }, countInBars, follow } });
  }

  /** Sends a transport patch; the backend answer becomes the new state. Returns false when it refused. */
  async setTransport(patch: TransportPatch): Promise<boolean> {
    const r = await this.backend.transportSet(patch);
    if (!r.ok) {
      this.set({ error: describeError(r.error) });
      return false;
    }
    await this.refreshTransport();
    return true;
  }

  /** A full fetch decides whether the project is big enough to follow the feed from now on. */
  private reseedMirror(project: Project, hash: string, undoDepth: number, redoDepth: number): void {
    if (countNotes(project) <= this.fullRefreshMax) {
      this.mirror?.dispose();
      this.mirror = null;
      return;
    }
    this.mirror ??= new ProjectMirror(this.backend, this.mirrorOptions);
    this.mirror.seed(project, hash, undoDepth, redoDepth);
  }

  /** Mirror counters (feeds applied, full fetches, audits) for composerDebug and tests; null while the store refetches. */
  mirrorStats(): ProjectMirror['stats'] | null {
    return this.mirror ? { ...this.mirror.stats } : null;
  }

  /** Follows one edit or push through the mirror; any gap falls back to a full refresh. */
  private async follow(hash: string, feed: Summary['feed'], depths?: { undo: number; redo: number }): Promise<void> {
    const mirror = this.mirror;
    if (!mirror) return this.refresh();
    const got = await mirror.sync(hash, feed, depths);
    if (!got) return this.refresh();
    if (got.project !== this.snap.project || got.hash !== this.snap.stateHash) {
      this.adopt(got.project, got.hash, got.undoDepth, got.redoDepth);
      await this.refreshTransport();
    }
  }

  private adopt(project: Project, hash: string, undoDepth: number, redoDepth: number): void {
    let track = this.snap.selectedTrack;
    // A clip that still exists keeps its place in the selection even if undo moved it to another track.
    const kept = findRegion(project, this.snap.selectedRegion);
    if (kept) track = kept.track.id;
    if (!track || !project.tracks.some((t) => t.id === track)) track = project.tracks[0]?.id ?? null;
    const tr = project.tracks.find((t) => t.id === track);
    let region = this.snap.selectedRegion;
    if (!region || !tr?.regions[region]) region = tr ? (Object.keys(tr.regions)[0] ?? null) : null;
    const found = findRegion(project, region);
    const regions = new Set<Id>();
    for (const id of this.snap.selectedRegions) if (findRegion(project, id)) regions.add(id);
    if (region) regions.add(region);
    const keep = new Set<Id>();
    if (found && this.snap.selectedNotes.size > 0) {
      const ids = new Set(sortedNotes(found.region).map((n) => n.id));
      for (const id of this.snap.selectedNotes) if (ids.has(id)) keep.add(id);
    }
    this.set({
      project,
      stateHash: hash,
      undoDepth,
      redoDepth,
      selectedTrack: track,
      selectedRegion: region,
      selectedRegions: regions,
      selectedNotes: keep,
      error: null,
    });
  }

  private async finish(r: Reply<Summary>, startedAt = performance.now()): Promise<Reply<Summary>> {
    if (!r.ok) {
      this.set({ error: describeError(r.error) });
      return r;
    }
    if (r.value.stateHash !== this.snap.stateHash) {
      if (this.mirror) await this.follow(r.value.stateHash, r.value.feed, { undo: r.value.undoDepth, redo: r.value.redoDepth });
      else await this.refresh();
    }
    recordEdit(performance.now() - startedAt);
    return r;
  }

  /** Runs a commit and announces its step label once the project has refreshed. */
  private async committed(r: Reply<Summary>, startedAt: number, fallback: string): Promise<Reply<Summary>> {
    const done = await this.finish(r, startedAt);
    if (done.ok) this.commit(done.value, fallback);
    return done;
  }

  async apply(command: Command): Promise<Reply<Summary>> {
    const t0 = performance.now();
    recordCommand(command.command);
    return this.committed(await this.backend.apply(command), t0, 'Edit');
  }

  /** Settles an edit the backend made on its own (accepting a proposal, running a recipe): refresh, then announce the step. */
  async settle(r: Reply<Summary>, fallback: string): Promise<Reply<Summary>> {
    return this.committed(r, performance.now(), fallback);
  }

  async group(label: string, commands: Command[]): Promise<Reply<Summary>> {
    if (commands.length === 0) return { ok: true, value: this.summaryNow() };
    const t0 = performance.now();
    recordCommand(label);
    return this.committed(await this.backend.group(label, commands), t0, label);
  }

  async undo(): Promise<void> {
    recordCommand('undo');
    const name = this.stepName(this.snap.undoDepth);
    const r = await this.finish(await this.backend.undo());
    if (r.ok && r.value.applied !== false) this.announce(`Undid ${name}`);
  }

  async redo(): Promise<void> {
    recordCommand('redo');
    const r = await this.finish(await this.backend.redo());
    if (r.ok && r.value.applied !== false) this.announce(`Redid ${this.stepName(r.value.undoDepth)}`);
  }

  /** One page of the undo history for the history panel. */
  history(offset: number, limit: number): Promise<Reply<HistoryPage>> {
    return this.backend.history(offset, limit);
  }

  /** Moves to a history position (count of applied steps) in one call; returns false when nothing moved. */
  async jumpTo(cursor: number, label: string): Promise<boolean> {
    const have = this.snap.undoDepth;
    if (cursor === have || cursor < 0 || cursor > have + this.snap.redoDepth) return false;
    const back = cursor < have;
    recordCommand(back ? 'undo steps' : 'redo steps');
    const r = await this.finish(await this.backend.jump(back ? 'undo' : 'redo', Math.abs(cursor - have)));
    if (!r.ok || r.value.applied === false) return false;
    this.announce(back ? `Went back to ${label}` : `Went forward to ${label}`);
    return true;
  }

  async load(project: Project): Promise<Reply<Summary>> {
    const r = await this.finish(await this.backend.load(project));
    return r;
  }

  /** Replaces the session with a project file kept by the backend; history starts over. */
  async openProject(path: string): Promise<Reply<Summary & FileResult>> {
    const r = await this.backend.openProject(path);
    if (!r.ok) {
      this.set({ error: describeError(r.error) });
      return r;
    }
    await this.refresh();
    return r;
  }

  async saveProject(path: string, overwrite: boolean): Promise<Reply<FileResult>> {
    const r = await this.backend.saveProject(path, overwrite);
    if (!r.ok) this.set({ error: describeError(r.error) });
    return r;
  }

  private summaryNow(): Summary {
    return {
      stateHash: this.snap.stateHash,
      undoDepth: this.snap.undoDepth,
      redoDepth: this.snap.redoDepth,
      undoLabel: null,
    };
  }

  clearError(): void {
    this.set({ error: null });
  }

  selectTrack(id: Id): void {
    const tr = this.snap.project.tracks.find((t) => t.id === id);
    if (!tr) return;
    const first = Object.keys(tr.regions)[0] ?? null;
    const keepRegion = this.snap.selectedRegion && tr.regions[this.snap.selectedRegion];
    this.set({
      selectedTrack: id,
      selectedRegion: keepRegion ? this.snap.selectedRegion : first,
      selectedRegions: new Set(keepRegion ? this.snap.selectedRegions : first ? [first] : []),
      selectedNotes: new Set(),
    });
  }

  selectRegion(trackId: Id, regionId: Id | null): void {
    this.set({
      selectedTrack: trackId,
      selectedRegion: regionId,
      selectedRegions: new Set(regionId ? [regionId] : []),
      selectedNotes: new Set(),
    });
  }

  /** Several clips at once; `primary` is the one the Notes editor follows. */
  selectRegions(trackId: Id, primary: Id | null, ids: Iterable<Id>): void {
    const all = new Set(ids);
    if (primary) all.add(primary);
    this.set({ selectedTrack: trackId, selectedRegion: primary, selectedRegions: all, selectedNotes: new Set() });
  }

  /** Speaks a navigation message in the live region (no command was committed). */
  say(text: string): void {
    this.announce(text);
  }

  selectNotes(ids: Iterable<Id>): void {
    this.set({ selectedNotes: new Set(ids) });
  }
}

/** Core step labels can be command ids such as add_note; speak them as plain words. */
export function plainLabel(label: string): string {
  if (!/^[a-z0-9]+(_[a-z0-9]+)+$/.test(label)) return label;
  const words = label.replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function describeError(e: BackendError): string {
  return plainError(e);
}
