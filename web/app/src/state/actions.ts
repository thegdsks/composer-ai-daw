// User level operations. Each one is built from Backend commands only (no UI only edits).
import { config } from '../config';
import { renderBlob } from '../audio/offline';
import { parseWav } from '../audio/wav';
import type { ComposerDebug } from '../debug';
import { newId } from '../ids';
import {
  INSTRUMENT_SINE,
  isMidi,
  notesOf,
  type Command,
  type Effect,
  type EffectKind,
  type EffectOwner,
  type Id,
  type Note,
  type Project,
  type Region,
} from '../model';
import { addEffectCommand, moveEffectCommand, paramCommand } from './effectsModel';
import { download, parseProject, projectFileName, safeFileName } from '../persistence';
import { plainError } from './errors';
import { findRegion, type Store } from './store';
import { clampReverb } from './soundPanel';
import { addNote, clampPan, clampVolume, defaultRegionLength, deleteNotes, duplicateClip, placeClip, repeatClips, setTempo, splitClip } from './commands';
import {
  Clipboard,
  barStartAt,
  clampRepeat,
  pasteClipsPlan,
  pasteNotesPlan,
  repeatLabel,
  repeatNotesPlan,
  snapshotClips,
  snapshotNotes,
  wordCount,
} from './clipboard';
import { overlapsOther } from '../roll/geometry';
import { meterSegments } from '../time';
import { chordPlan, type ChordMode } from './chordPlan';
import { loadTemplate, resolveTemplate, type TemplateId } from '../templates';

/** Where a clip goes. A null track means "a new track at the end" (dropped below the last lane). */
export interface ClipPlacement {
  regionId: Id;
  trackId: Id | null;
  start: number;
}

function clipsWord(n: number): string {
  return n === 1 ? 'clip' : `${n} clips`;
}

/** Maps a pitch for Stay in key; identity when it is off. */
export type PitchKeeper = (pitch: number) => number;
const keepAll: PitchKeeper = (p) => p;

export class Actions {
  /** Internal clipboard for notes and clips (not mirrored to the system clipboard). */
  readonly clipboard = new Clipboard();
  /** Set by the Repeat dialog; resolves with the number of copies or null when cancelled. */
  private askRepeat: ((what: string) => Promise<number | null>) | null = null;

  /** The Repeat dialog registers here; pass null when it unmounts. */
  setRepeatPrompt(ask: ((what: string) => Promise<number | null>) | null): void {
    this.askRepeat = ask;
  }

  constructor(
    private readonly store: Store,
    private readonly module: WebAssembly.Module,
    private readonly debug: ComposerDebug,
    private readonly notify: (message: string) => void,
  ) {}

  private get project(): Project {
    return this.store.getSnapshot().project;
  }

  private meterAt(tick: number): { numerator: number; denominator: number } {
    const segs = meterSegments(this.project.tempo_map);
    let sig = segs[0]?.sig ?? { numerator: 4, denominator: 4 };
    for (const s of segs) if (s.tick <= tick) sig = s.sig;
    return sig;
  }

  async rename(name: string): Promise<void> {
    const clean = name.trim();
    if (clean && clean !== this.project.name) await this.store.apply({ command: 'set_project_name', name: clean });
  }

  async setBpm(bpm: number): Promise<void> {
    if (Number.isFinite(bpm)) await this.store.apply(setTempo(bpm));
  }

  async addTrack(): Promise<void> {
    const id = newId();
    const region = newId();
    const n = this.project.tracks.length + 1;
    const length = defaultRegionLength(this.meterAt(0));
    const r = await this.store.group('Add track', [
      { command: 'add_track', id, name: `Track ${n}`, kind: 'instrument', index: null },
      { command: 'set_instrument', track: id, instrument: INSTRUMENT_SINE },
      { command: 'add_region', id: region, track: id, start: 0, length, kind: 'midi' },
    ]);
    if (r.ok) this.store.selectRegion(id, region);
  }

  /**
   * Replaces the open piece with a template as one step. Existing tracks are deleted inside the
   * same group, so a single Undo brings the old piece back. Returns false (and says why) on failure.
   */
  async applyTemplate(id: TemplateId): Promise<boolean> {
    try {
      const { template, commands, ids } = resolveTemplate(loadTemplate(id));
      const clear: Command[] = this.project.tracks.map((t) => ({ command: 'delete_track', track: t.id }));
      const r = await this.store.group(`Start from ${template.name}`, [...clear, ...commands]);
      if (!r.ok) {
        this.notify(`Could not start from ${template.name}: ${plainError(r.error)}`);
        return false;
      }
      const track = ids.get('$t1');
      const region = ids.get('$r1');
      if (track && region) this.store.selectRegion(track, region);
      return true;
    } catch (e) {
      this.notify(`Could not start from the template: ${e instanceof Error ? e.message : 'unknown error'}`);
      return false;
    }
  }

  async renameTrack(track: Id, name: string): Promise<void> {
    const clean = name.trim();
    const cur = this.project.tracks.find((t) => t.id === track);
    if (clean && cur && cur.name !== clean) await this.store.apply({ command: 'rename_track', track, name: clean });
  }

  /** Picks a basic instrument. A catalogue sound would still win in the engine, so it is cleared in the same step. */
  async setInstrument(track: Id, instrument: Id | null): Promise<void> {
    const cur = this.project.tracks.find((t) => t.id === track);
    if (cur?.sound) {
      await this.store.group('Change instrument', [
        { command: 'set_instrument', track, instrument },
        { command: 'set_track_sound', track, sound: null },
      ]);
      return;
    }
    await this.store.apply({ command: 'set_instrument', track, instrument });
  }

  /** Gives a track a catalogue sound (one undo step); null goes back to its instrument. */
  async setSound(track: Id, sound: string | null): Promise<boolean> {
    const cur = this.project.tracks.find((t) => t.id === track);
    if (!cur || (cur.sound ?? null) === sound) return false;
    const r = await this.store.apply({ command: 'set_track_sound', track, sound });
    return r.ok;
  }

  async deleteTrack(track: Id): Promise<void> {
    await this.store.apply({ command: 'delete_track', track });
  }

  /** Adds an empty region at a tick of a track; returns its id when it worked. */
  async addRegion(track: Id, start: number, length?: number): Promise<Id | null> {
    const id = newId();
    const len = length ?? defaultRegionLength(this.meterAt(start));
    const r = await this.store.apply({ command: 'add_region', id, track, start, length: len, kind: 'midi' });
    if (!r.ok) return null;
    this.store.selectRegion(track, id);
    return id;
  }

  async deleteRegion(region: Id): Promise<void> {
    await this.store.apply({ command: 'delete_region', region });
  }

  /** A note of one grid step at the playhead or the start of the selected region. */
  async addNoteAt(region: Id, pitch: number, start: number, duration: number): Promise<Id | null> {
    const id = newId();
    const r = await this.store.apply(addNote(id, region, pitch, start, duration));
    return r.ok ? id : null;
  }

  async exportWav(): Promise<void> {
    const t0 = performance.now();
    const blob = await this.store.backend.engineBlob(config.exportSampleRate);
    if (!blob.ok) return this.notify(`Export failed: ${blob.error.message}`);
    try {
      const out = await renderBlob(this.module, blob.value);
      const name = safeFileName(this.project.name, 'wav');
      this.debug.lastExport = {
        name,
        bytes: out.wav.length,
        header: parseWav(out.wav),
        peak: out.peak,
        renderMs: Math.round(performance.now() - t0),
      };
      download(name, out.wav, 'audio/wav');
      this.notify(`Exported ${name}`);
    } catch (e) {
      this.notify(`Export failed: ${e instanceof Error ? e.message : 'unknown error'}`);
    }
  }

  exportJson(): void {
    const name = safeFileName(this.project.name, 'composer.json');
    download(name, JSON.stringify(this.project, null, 2), 'application/json');
    this.notify(`Saved ${name}`);
  }

  /** Returns true when the file was loaded as the open project. */
  async importJson(file: File): Promise<boolean> {
    const project = parseProject(await file.text());
    if (!project) {
      this.notify('That file is not a Composer AI DAW project.');
      return false;
    }
    const r = await this.store.load(project);
    if (!r.ok) {
      this.notify(`Could not open the project: ${r.error.message}`);
      return false;
    }
    this.notify(`Opened ${project.name}`);
    return true;
  }

  /** Saves into the daemon files dir. Returns the error text, or null when it worked. */
  async saveToDaemon(name: string, overwrite: boolean): Promise<string | null> {
    const path = projectFileName(name);
    const r = await this.store.saveProject(path, overwrite);
    if (!r.ok) return plainError(r.error);
    this.notify(`Saved ${r.value.path}`);
    return null;
  }

  /** Opens a project file from the daemon files dir. Returns the error text, or null. */
  async openFromDaemon(name: string): Promise<string | null> {
    const path = projectFileName(name);
    const r = await this.store.openProject(path);
    if (!r.ok) return plainError(r.error);
    this.notify(`Opened ${r.value.name ?? path}`);
    return null;
  }

  async setVolume(track: Id, volume: number): Promise<void> {
    await this.store.apply({ command: 'set_track_volume', track, volume: clampVolume(volume) });
  }

  async setPan(track: Id, pan: number): Promise<void> {
    await this.store.apply({ command: 'set_track_pan', track, pan: clampPan(pan) });
  }

  async setReverbSend(track: Id, send: number): Promise<void> {
    await this.store.apply({ command: 'set_track_reverb_send', track, send: clampReverb(send) });
  }

  async setMasterVolume(volume: number): Promise<void> {
    await this.store.apply({ command: 'set_master_volume', volume: clampVolume(volume) });
  }

  async setMasterReverb(size: number, mix: number): Promise<void> {
    await this.store.apply({ command: 'set_master_reverb', size: clampReverb(size), mix: clampReverb(mix) });
  }

  async setMute(track: Id, mute: boolean): Promise<void> {
    await this.store.apply({ command: 'set_track_mute', track, mute });
  }

  /** Appends a device with default settings (or inserts at `index`); returns its id, or null when refused. */
  async addEffect(owner: EffectOwner, kind: EffectKind, index: number | null = null): Promise<Id | null> {
    const id = newId();
    const r = await this.store.apply(addEffectCommand(id, owner, kind, index));
    return r.ok ? id : null;
  }

  async removeEffect(effect: Id): Promise<void> {
    await this.store.apply({ command: 'remove_effect', effect });
  }

  async moveEffect(effect: Id, index: number): Promise<void> {
    await this.store.apply(moveEffectCommand(effect, index));
  }

  async setEffectBypass(effect: Id, bypass: boolean): Promise<void> {
    await this.store.apply({ command: 'set_effect_bypass', effect, bypass });
  }

  /** One release is one command; a value already set sends nothing. */
  async setEffectParam(effect: Effect, name: string, value: number): Promise<void> {
    const c = paramCommand(effect, name, value);
    if (c) await this.store.apply(c);
  }

  async setSolo(track: Id, solo: boolean): Promise<void> {
    await this.store.apply({ command: 'set_track_solo', track, solo });
  }

  /** Region drags report here once, on release: one command, one undo step. */
  async moveRegion(region: Id, start: number): Promise<void> {
    await this.store.apply({ command: 'move_region', region, start });
  }

  async resizeRegion(region: Id, length: number): Promise<void> {
    await this.store.apply({ command: 'resize_region', region, length });
  }

  /** A new instrument track, as commands, for drops below the last lane. */
  private newTrackCommands(id: Id): Command[] {
    return [
      { command: 'add_track', id, name: `Track ${this.project.tracks.length + 1}`, kind: 'instrument', index: null },
      { command: 'set_instrument', track: id, instrument: INSTRUMENT_SINE },
    ];
  }

  /** Says so when a landed clip shares time with another one (allowed, but easy to miss). */
  private warnOverlap(regionIds: readonly Id[]): void {
    for (const id of regionIds) {
      const found = findRegion(this.project, id);
      if (found && overlapsOther(Object.values(found.track.regions), id)) {
        this.notify('That clip overlaps another one. Press Ctrl Z to undo.');
        return;
      }
    }
  }

  /** Moves clips (start and track) as one step: one gesture, one undo. */
  async moveClips(placements: readonly ClipPlacement[], anchor: Id): Promise<void> {
    const cmds: Command[] = [];
    const fresh = placements.some((p) => p.trackId === null) ? newId() : null;
    if (fresh) cmds.push(...this.newTrackCommands(fresh));
    const dest = new Map<Id, Id>();
    for (const p of placements) {
      const found = findRegion(this.project, p.regionId);
      const to = p.trackId ?? fresh;
      if (!found || !to) continue;
      dest.set(p.regionId, to);
      cmds.push(...placeClip(found.region, found.track.id, to, p.start));
    }
    if (cmds.length === 0) return;
    const r = await this.store.group(`Move ${clipsWord(placements.length)}`, cmds);
    if (!r.ok) return;
    this.store.selectRegions(dest.get(anchor) ?? this.store.getSnapshot().selectedTrack ?? '', anchor, dest.keys());
    this.warnOverlap([...dest.keys()]);
  }

  /** Copies clips to the given places as one step; the copies become the selection. */
  async copyClips(placements: readonly ClipPlacement[], anchor: Id): Promise<void> {
    const cmds: Command[] = [];
    const fresh = placements.some((p) => p.trackId === null) ? newId() : null;
    if (fresh) cmds.push(...this.newTrackCommands(fresh));
    const copies = new Map<Id, Id>();
    const trackOf = new Map<Id, Id>();
    for (const p of placements) {
      const found = findRegion(this.project, p.regionId);
      if (!found) continue;
      const to = p.trackId ?? fresh;
      const cmd = duplicateClip(found.region, p.start, to === found.track.id ? null : to, newId);
      if (cmd.command !== 'duplicate_region') continue;
      cmds.push(cmd);
      copies.set(p.regionId, cmd.new_region);
      trackOf.set(cmd.new_region, to ?? found.track.id);
    }
    if (cmds.length === 0) return;
    const r = await this.store.group(`Copy ${clipsWord(placements.length)}`, cmds);
    if (!r.ok) return;
    const primary = copies.get(anchor) ?? [...copies.values()][0] ?? null;
    if (primary) this.store.selectRegions(trackOf.get(primary) ?? '', primary, copies.values());
    this.warnOverlap([...copies.values()]);
  }

  /** Cuts every listed clip that the tick falls strictly inside; returns how many were cut. */
  async splitClips(regionIds: readonly Id[], at: number): Promise<number> {
    const cmds: Command[] = [];
    for (const id of regionIds) {
      const found = findRegion(this.project, id);
      const cmd = found ? splitClip(found.region, at, newId) : null;
      if (cmd) cmds.push(cmd);
    }
    if (cmds.length === 0) {
      this.notify('Move the playhead inside a selected clip to split it.');
      return 0;
    }
    const r = await this.store.group(`Split ${clipsWord(cmds.length)}`, cmds);
    return r.ok ? cmds.length : 0;
  }

  async deleteClips(regionIds: readonly Id[]): Promise<void> {
    const cmds: Command[] = regionIds.map((region) => ({ command: 'delete_region', region }));
    await this.store.group(`Delete ${clipsWord(cmds.length)}`, cmds);
  }

  /** Region under selection, used by keyboard shortcuts. */
  selectedRegion(): { trackId: Id; regionId: Id } | null {
    const snap = this.store.getSnapshot();
    const found = findRegion(snap.project, snap.selectedRegion);
    return found ? { trackId: found.track.id, regionId: found.region.id } : null;
  }
  // ---- copy, cut, paste, duplicate and repeat (U11) ----

  private pickedClips(ids: readonly Id[]): { track: Id; region: Region }[] {
    const out: { track: Id; region: Region }[] = [];
    for (const id of ids) {
      const found = findRegion(this.project, id);
      if (found) out.push({ track: found.track.id, region: found.region });
    }
    return out;
  }

  private pickedNotes(regionId: Id, noteIds: ReadonlySet<Id>): { region: Region; notes: Note[] } | null {
    const found = findRegion(this.project, regionId);
    if (!found || !isMidi(found.region)) return null;
    const notes = notesOf(found.region).filter((n) => noteIds.has(n.id));
    return notes.length > 0 ? { region: found.region, notes } : null;
  }

  /** Bar line at or before a tick, the place a paste lands. */
  pasteTick(tick: number): number {
    return barStartAt(this.project.tempo_map, Math.max(0, Math.round(tick)));
  }

  /** Ctrl C on notes. Returns false when nothing was selected. */
  copyNotes(regionId: Id, noteIds: ReadonlySet<Id>): boolean {
    const picked = this.pickedNotes(regionId, noteIds);
    if (!picked) {
      this.store.say('Select some notes first');
      return false;
    }
    this.clipboard.set({ kind: 'notes', notes: snapshotNotes(this.project.tempo_map, picked.region, picked.notes) });
    this.store.say(`Copied ${wordCount(picked.notes.length, 'note')}`);
    return true;
  }

  /** Ctrl X on notes: copy, then delete, as one step. */
  async cutNotes(regionId: Id, noteIds: ReadonlySet<Id>): Promise<void> {
    const picked = this.pickedNotes(regionId, noteIds);
    if (!picked || !this.copyNotes(regionId, noteIds)) return;
    await this.store.group(`Cut ${wordCount(picked.notes.length, 'note')}`, deleteNotes(regionId, picked.notes));
  }

  /** Ctrl V into a clip at a tick (snapped to the bar); notes copied from clips are placed by bar offset. */
  async pasteNotes(regionId: Id, tick: number, keep: PitchKeeper = keepAll): Promise<boolean> {
    const clip = this.clipboard.get();
    const found = findRegion(this.project, regionId);
    if (!found || !isMidi(found.region)) {
      this.store.say('Open a clip with notes to paste into');
      return false;
    }
    if (clip?.kind !== 'notes') {
      this.store.say(clip ? 'Copy some notes first. Clips paste in the arrangement' : 'Nothing copied yet');
      return false;
    }
    const at = Math.max(this.pasteTick(tick), found.region.start);
    const plan = pasteNotesPlan(this.project.tempo_map, found.region, clip.notes, at, newId, keep);
    const r = await this.store.group(`Paste ${wordCount(clip.notes.length, 'note')}`, plan.commands);
    if (r.ok) this.store.selectNotes(plan.ids);
    return r.ok;
  }

  /** Chord strip: puts a chord into the open clip as one undo step (replaces the bar's notes in bar mode). */
  async placeChord(regionId: Id, name: string, pitches: readonly number[], playhead: number, mode: ChordMode, grid: number): Promise<boolean> {
    const found = findRegion(this.project, regionId);
    if (!found || !isMidi(found.region)) {
      this.store.say('Open a clip with notes to add a chord');
      return false;
    }
    const plan = chordPlan(this.project.tempo_map, found.region, playhead, mode, pitches, grid, config.liveVelocity, newId);
    const label = mode === 'bar' ? `Chord ${name} in bar ${plan.bar}` : `Add chord ${name} at bar ${plan.bar}`;
    const r = await this.store.group(label, plan.commands);
    if (r.ok) this.store.selectNotes(plan.ids);
    return r.ok;
  }

  /** Ctrl D on notes: copies right after the selection (one grid step multiple later). */
  async duplicateNotes(regionId: Id, noteIds: ReadonlySet<Id>, grid: number, times = 1, keep: PitchKeeper = keepAll): Promise<void> {
    const picked = this.pickedNotes(regionId, noteIds);
    if (!picked) return void this.store.say('Select some notes first');
    const n = clampRepeat(times);
    const plan = repeatNotesPlan(this.project.tempo_map, picked.region, picked.notes, n, grid, newId, keep);
    const label = times > 1 ? repeatLabel(n) : `Duplicate ${wordCount(picked.notes.length, 'note')}`;
    const r = await this.store.group(label, plan.commands);
    if (r.ok) this.store.selectNotes(plan.ids);
  }

  /** Repeat x N on notes: asks for N first. */
  async repeatNotes(regionId: Id, noteIds: ReadonlySet<Id>, grid: number, keep: PitchKeeper = keepAll): Promise<void> {
    if (!this.pickedNotes(regionId, noteIds)) return void this.store.say('Select some notes first');
    const n = this.askRepeat ? await this.askRepeat('notes') : null;
    if (n !== null) await this.duplicateNotes(regionId, noteIds, grid, n, keep);
  }

  /** Ctrl C on clips. */
  copyClipsToClipboard(ids: readonly Id[]): boolean {
    const shapes = snapshotClips(this.pickedClips(ids));
    if (shapes.length === 0) {
      this.store.say(ids.length === 0 ? 'Select a clip first' : 'Audio clips cannot be copied yet. Use Duplicate');
      return false;
    }
    this.clipboard.set({ kind: 'clips', clips: shapes });
    this.store.say(`Copied ${wordCount(shapes.length, 'clip')}`);
    return true;
  }

  /** Ctrl X on clips: copy, then delete, as one step. */
  async cutClips(ids: readonly Id[]): Promise<void> {
    if (!this.copyClipsToClipboard(ids)) return;
    const live = this.pickedClips(ids).filter((c) => isMidi(c.region));
    const cmds: Command[] = live.map((c) => ({ command: 'delete_region', region: c.region.id }));
    await this.store.group(`Cut ${wordCount(cmds.length, 'clip')}`, cmds);
  }

  /** Ctrl V in the arrangement: clips land at the bar line, notes go into the selected clip. */
  async pasteInArrangement(tick: number, keep: PitchKeeper = keepAll): Promise<boolean> {
    const clip = this.clipboard.get();
    if (!clip) {
      this.store.say('Nothing copied yet');
      return false;
    }
    const snap = this.store.getSnapshot();
    if (clip.kind === 'notes') return this.pasteNotes(snap.selectedRegion ?? '', tick, keep);
    const fallback = snap.selectedTrack ?? this.project.tracks[0]?.id;
    if (!fallback) {
      this.store.say('Add a track to paste into');
      return false;
    }
    const at = this.pasteTick(tick);
    const exists = (id: Id): boolean => this.project.tracks.some((t) => t.id === id);
    const plan = pasteClipsPlan(clip.clips, at, exists, fallback, newId);
    const r = await this.store.group(`Paste ${wordCount(clip.clips.length, 'clip')}`, plan.commands);
    if (!r.ok) return false;
    const primary = plan.ids[0] ?? null;
    this.store.selectRegions(plan.tracks[0] ?? fallback, primary, plan.ids);
    this.warnOverlap(plan.ids);
    return true;
  }

  /** Ctrl D on clips: the selection is copied right after itself, `times` blocks in a row. */
  async duplicateClips(ids: readonly Id[], times = 1): Promise<void> {
    const picked = this.pickedClips(ids);
    if (picked.length === 0) return void this.store.say('Select a clip first');
    const n = clampRepeat(times);
    const cmds = repeatClips(picked.map((c) => c.region), n, newId);
    const label = times > 1 ? repeatLabel(n) : picked.length === 1 ? 'Duplicate clip' : `Duplicate ${picked.length} clips`;
    const r = await this.store.group(label, cmds);
    if (!r.ok) return;
    const copies = cmds.flatMap((c) => (c.command === 'duplicate_region' ? [c.new_region] : []));
    const last = copies.slice(-picked.length);
    const found = findRegion(this.project, last[0] ?? null);
    if (found) this.store.selectRegions(found.track.id, last[0] ?? null, last);
    this.warnOverlap(copies);
  }

  /** Repeat x N on clips: asks for N first. */
  async repeatClipsAsk(ids: readonly Id[]): Promise<void> {
    if (this.pickedClips(ids).length === 0) return void this.store.say('Select a clip first');
    const n = this.askRepeat ? await this.askRepeat('clips') : null;
    if (n !== null) await this.duplicateClips(ids, n);
  }
}
