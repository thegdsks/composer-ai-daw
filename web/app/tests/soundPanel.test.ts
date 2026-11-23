import { describe, expect, it } from 'vitest';
import type { SoundEntry } from '../src/backend/types';
import { starterCommands } from '../src/fixtures';
import { INSTRUMENT_SAW, INSTRUMENT_SINE, type Project, type Track } from '../src/model';
import { builtinCatalogue } from '../src/sounds/builtin';
import { Store } from '../src/state/store';
import {
  clampReverb,
  instrumentLabel,
  masterOf,
  neighbourSound,
  reverbSendOf,
  reverbText,
  soundLabel,
  trackSendText,
} from '../src/state/soundPanel';
import { counterIds, newActions, newBackend } from './helpers';

const entry = (over: Partial<SoundEntry>): SoundEntry => ({
  id: 'x',
  name: 'X',
  kind: 'builtin',
  licence: 'CC0 1.0',
  instruments: '',
  bundle: 'yes',
  ...over,
});

const track = (over: Partial<Track>): Track => ({
  id: 't',
  name: 'Track 1',
  kind: 'instrument',
  instrument: INSTRUMENT_SINE,
  regions: {},
  ...over,
});

describe('sound panel values', () => {
  it('clamps reverb to 0..1000 and rounds', () => {
    expect(clampReverb(-5)).toBe(0);
    expect(clampReverb(1500)).toBe(1000);
    expect(clampReverb(333.6)).toBe(334);
  });

  it('reads absent fields as defaults', () => {
    expect(reverbSendOf(track({}))).toBe(0);
    expect(reverbSendOf(track({ reverb_send: 250 }))).toBe(250);
    const p = { id: 'p', name: 'p', tempo_map: { tempos: {}, meters: {} }, tracks: [] } satisfies Project;
    expect(masterOf(p)).toEqual({ volume: 1000, reverb_size: 500, reverb_mix: 0 });
    expect(masterOf({ ...p, master: { reverb_mix: 300 } }).reverb_mix).toBe(300);
  });

  it('uses plain labels', () => {
    expect(reverbText(0)).toBe('Off');
    expect(reverbText(500)).toBe('50 percent');
    expect(trackSendText(0)).toBe('None');
    expect(instrumentLabel(track({ instrument: INSTRUMENT_SAW }))).toBe('Saw');
    expect(instrumentLabel(track({ instrument: null }))).toBe('No instrument');
    expect(instrumentLabel(track({ sound: 'a' }))).toBe('Catalogue sound');
    expect(soundLabel(track({}), [])).toBe('Basic instrument');
    expect(soundLabel(track({ sound: 'a' }), [entry({ id: 'a', name: 'Warm' })])).toBe('Warm');
    expect(soundLabel(track({ sound: 'gone' }), [])).toBe('Custom sound');
  });
});

describe('neighbourSound', () => {
  const pad = (id: string): SoundEntry => entry({ id, name: id, instruments: 'pad' });
  const list = [pad('pad-a'), entry({ id: 'bass-a', name: 'bass-a', instruments: 'bass' }), pad('pad-b'), pad('pad-c')];

  it('steps inside the category and wraps both ways', () => {
    expect(neighbourSound(list, 'pad-a', 1)?.id).toBe('pad-b');
    expect(neighbourSound(list, 'pad-c', 1)?.id).toBe('pad-a');
    expect(neighbourSound(list, 'pad-a', -1)?.id).toBe('pad-c');
  });

  it('skips hidden and sample pack entries', () => {
    const mixed = [pad('pad-a'), entry({ id: 'pad-x', kind: 'sfz', bundle: 'yes', instruments: 'pad' }), pad('pad-b'), entry({ id: 'pad-h', kind: 'plugin', instruments: 'pad' })];
    expect(neighbourSound(mixed, 'pad-a', 1)?.id).toBe('pad-b');
    expect(neighbourSound(mixed, 'pad-b', 1)?.id).toBe('pad-a');
  });

  it('starts from the first or last sound when none is set, and has none for a lone sound', () => {
    expect(neighbourSound(list, null, 1)?.id).toBe('pad-a');
    expect(neighbourSound(list, null, -1)?.id).toBe('pad-c');
    expect(neighbourSound([pad('only')], 'only', 1)).toBeNull();
    expect(neighbourSound([], null, 1)).toBeNull();
  });

  it('works on the built in catalogue', () => {
    const entries = builtinCatalogue().entries;
    const first = entries[0];
    expect(first).toBeDefined();
    expect(neighbourSound(entries, null, 1)).not.toBeNull();
  });
});

describe('reverb and master commands', () => {
  it('each change is one undo step and undo restores', async () => {
    const store = new Store(newBackend());
    await store.refresh();
    await store.group('starter', starterCommands(counterIds()));
    const actions = newActions(store);
    const id = store.getSnapshot().project.tracks[0]?.id ?? '';
    const depth = store.getSnapshot().undoDepth;
    await actions.setReverbSend(id, 400);
    await actions.setMasterVolume(800);
    await actions.setMasterReverb(600, 300);
    const p = store.getSnapshot().project;
    expect(reverbSendOf(p.tracks[0]!)).toBe(400);
    expect(masterOf(p)).toEqual({ volume: 800, reverb_size: 600, reverb_mix: 300 });
    expect(store.getSnapshot().undoDepth).toBe(depth + 3);
    await store.undo();
    await store.undo();
    await store.undo();
    const q = store.getSnapshot().project;
    expect(reverbSendOf(q.tracks[0]!)).toBe(0);
    expect(masterOf(q)).toEqual({ volume: 1000, reverb_size: 500, reverb_mix: 0 });
  });
});
