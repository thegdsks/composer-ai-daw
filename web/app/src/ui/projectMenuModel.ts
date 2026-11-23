export type ProjectMenuId = 'new' | 'open' | 'save' | 'template' | 'audio' | 'midi' | 'rename' | 'plugin' | 'unplugin' | 'help';

export interface ProjectMenuState {
  trackCount: number;
  noteCount: number;
  busy: boolean;
  /** A web plugin is loaded on the master output. */
  pluginLoaded?: boolean;
}

export interface ProjectMenuEntry {
  id: ProjectMenuId;
  label: string;
  enabled: boolean;
  /** Plain reason shown when the entry is off. */
  reason?: string;
}

/** Which entries are on, from project state alone, so the rule is testable without a page. */
export function projectMenuEntries(s: ProjectMenuState): ProjectMenuEntry[] {
  const silent = s.trackCount === 0 || s.noteCount === 0;
  return [
    { id: 'new', label: 'New project', enabled: !s.busy },
    { id: 'open', label: 'Open file', enabled: !s.busy },
    { id: 'save', label: 'Save to file', enabled: !s.busy },
    { id: 'template', label: 'Save as template', enabled: false, reason: 'Your own templates are not built yet' },
    {
      id: 'audio',
      label: 'Export audio',
      enabled: !s.busy && !silent,
      ...(silent ? { reason: 'Add a note first, there is nothing to render' } : {}),
    },
    { id: 'midi', label: 'Export MIDI', enabled: false, reason: 'MIDI export is not available in the browser yet' },
    { id: 'rename', label: 'Rename project', enabled: !s.busy },
    { id: 'plugin', label: 'Add plugin file', enabled: !s.busy },
    ...(s.pluginLoaded ? [{ id: 'unplugin' as const, label: 'Remove plugin', enabled: true }] : []),
    { id: 'help', label: 'Keys and help', enabled: true },
  ];
}
