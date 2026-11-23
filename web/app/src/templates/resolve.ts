// Turns template command data into commands with real ids. A placeholder is any string
// that looks like `$name`; every use of one name in a template gets the same new id.
import type { Command } from '../model';
import { newId } from '../ids';

export interface TemplateLoop {
  start: number;
  end: number;
}

export interface Template {
  name: string;
  description: string;
  /** Beats per minute at tick 0 (the commands also carry it as set_tempo). */
  tempo: number;
  key: string;
  ticks_per_quarter: number;
  /** Range the client loops on first play (client state, not a command). */
  loop: TemplateLoop;
  /** Commands with `$name` placeholders in id fields. */
  commands: unknown[];
}

const PLACEHOLDER = /^\$[a-z][a-z0-9_]*$/;

export function isPlaceholder(value: unknown): value is string {
  return typeof value === 'string' && PLACEHOLDER.test(value);
}

function substitute(value: unknown, ids: Map<string, string>, nextId: () => string): unknown {
  if (isPlaceholder(value)) {
    let id = ids.get(value);
    if (id === undefined) {
      id = nextId();
      ids.set(value, id);
    }
    return id;
  }
  if (Array.isArray(value)) return value.map((v) => substitute(v, ids, nextId));
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) out[k] = substitute(v, ids, nextId);
    return out;
  }
  return value;
}

export interface ResolvedTemplate {
  template: Template;
  commands: Command[];
  /** Placeholder name to the id it received, so a client can find the first clip. */
  ids: ReadonlyMap<string, string>;
}

export function resolveTemplate(template: Template, nextId: () => string = newId): ResolvedTemplate {
  const ids = new Map<string, string>();
  const commands = template.commands.map((c) => substitute(c, ids, nextId) as Command);
  return { template, commands, ids };
}
