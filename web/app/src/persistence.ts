// M1 persistence: autosave to localStorage (may be absent or full) and project JSON files.
import { config } from './config';
import type { Project } from './model';

/** Loose shape check; the editor does the full validation when the project is loaded. */
export function looksLikeProject(v: unknown): v is Project {
  if (typeof v !== 'object' || v === null) return false;
  const p = v as Record<string, unknown>;
  return (
    typeof p.id === 'string' &&
    typeof p.name === 'string' &&
    Array.isArray(p.tracks) &&
    typeof p.tempo_map === 'object' &&
    p.tempo_map !== null
  );
}

export function parseProject(text: string): Project | null {
  try {
    const v: unknown = JSON.parse(text);
    return looksLikeProject(v) ? v : null;
  } catch {
    return null;
  }
}

export interface SaveStatus {
  /** Time of the last good autosave, null before the first one. */
  at: Date | null;
  /** True when the last attempt was refused (storage full or blocked). */
  failed: boolean;
}

let saveStatus: SaveStatus = { at: null, failed: false };
const saveListeners = new Set<() => void>();

export const getSaveStatus = (): SaveStatus => saveStatus;
export function subscribeSaveStatus(l: () => void): () => void {
  saveListeners.add(l);
  return () => saveListeners.delete(l);
}

export function saveLocal(project: Project): boolean {
  let ok = true;
  try {
    localStorage.setItem(config.storageKey, JSON.stringify(project));
  } catch {
    ok = false;
  }
  saveStatus = { at: ok ? new Date() : saveStatus.at, failed: !ok };
  for (const l of saveListeners) l();
  return ok;
}

export function loadLocal(): Project | null {
  try {
    const text = localStorage.getItem(config.storageKey);
    return text ? parseProject(text) : null;
  } catch {
    return null;
  }
}

export function clearLocal(): void {
  try {
    localStorage.removeItem(config.storageKey);
  } catch {
    // Storage can be blocked; nothing to clear then.
  }
}

export function safeFileName(name: string, extension: string): string {
  const base = name.trim().replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '');
  return `${base || 'project'}.${extension}`;
}

/** Daemon project file name: a plain name gets the extension the daemon requires. */
export function projectFileName(name: string): string {
  const clean = name.trim();
  const ext = `.${config.projectExtension}`;
  return clean.toLowerCase().endsWith(ext) ? clean : `${clean}${ext}`;
}

/** Hands bytes to the browser as a download. */
export function download(name: string, data: BlobPart, type: string): void {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}
