// The one suggestion shown under the top bar. A fixed rule list, first match wins; it only
// reads the project, so the UI can send the matching command and nothing else decides here.
import { config } from './config';
import { notesOf, type Id, type Project } from './model';
import { findRegion, projectEnd } from './state/store';

export type NextActionId = 'assist' | 'start' | 'empty-clip' | 'pick-sound' | 'repeat' | 'export';

export interface NextAction {
  id: NextActionId;
  text: string;
  button: string;
}

export function nextAction(project: Project, selectedRegion: Id | null, exported: boolean): NextAction | null {
  if (project.tracks.length === 0) {
    return { id: 'start', text: 'Nothing here yet.', button: 'Pick a starting point' };
  }
  const selected = findRegion(project, selectedRegion);
  if (selected && notesOf(selected.region).length === 0) {
    return { id: 'empty-clip', text: `This clip on ${selected.track.name} is empty.`, button: 'Draw a note' };
  }
  const plain = project.tracks.find((t) => !t.sound);
  if (plain) {
    return { id: 'pick-sound', text: `${plain.name} has the basic sound.`, button: 'Pick a sound' };
  }
  const clips = project.tracks.reduce((n, t) => n + Object.keys(t.regions).length, 0);
  if (clips === 1) {
    return { id: 'repeat', text: 'One clip so far.', button: 'Repeat it' };
  }
  const bars = projectEnd(project) / (config.ticksPerQuarter * 4);
  if (!exported && bars >= config.nextActionLongBars) {
    return { id: 'export', text: 'Long piece, not saved as audio yet.', button: 'Export WAV' };
  }
  return null;
}
