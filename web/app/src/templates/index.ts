// Bundled starter templates (repo folder templates/) with a checked loader. No UI here.
import { config } from '../config';
import type { Template, TemplateLoop } from './resolve';

export { resolveTemplate, isPlaceholder } from './resolve';
export type { Template, TemplateLoop, ResolvedTemplate } from './resolve';

export const TEMPLATE_IDS = ['recreate', 'cinematic', 'calm-hour', 'blank'] as const;
export type TemplateId = (typeof TEMPLATE_IDS)[number];

const files = import.meta.glob<unknown>('../../../../templates/*.json', { eager: true, import: 'default' });

function isRecord(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

/** Checks the shape of parsed JSON; throws a plain message naming the first problem. */
export function parseTemplate(raw: unknown): Template {
  if (!isRecord(raw)) throw new Error('template is not an object');
  const { name, description, tempo, key, ticks_per_quarter: tpq, loop, commands } = raw;
  if (typeof name !== 'string' || name === '') throw new Error('template name is missing');
  if (typeof description !== 'string') throw new Error(`${name}: description is missing`);
  if (typeof tempo !== 'number' || tempo <= 0) throw new Error(`${name}: tempo must be a positive number`);
  if (typeof key !== 'string') throw new Error(`${name}: key is missing`);
  if (tpq !== config.ticksPerQuarter) throw new Error(`${name}: ticks_per_quarter does not match the engine`);
  if (!isRecord(loop) || typeof loop.start !== 'number' || typeof loop.end !== 'number' || loop.end <= loop.start) {
    throw new Error(`${name}: loop needs start and end`);
  }
  if (!Array.isArray(commands) || !commands.every((c) => isRecord(c) && typeof c.command === 'string')) {
    throw new Error(`${name}: commands must be a list of command objects`);
  }
  const range: TemplateLoop = { start: loop.start, end: loop.end };
  return { name, description, tempo, key, ticks_per_quarter: tpq, loop: range, commands };
}

export function loadTemplate(id: TemplateId): Template {
  const hit = Object.entries(files).find(([path]) => path.endsWith(`/${id}.json`));
  if (!hit) throw new Error(`template ${id} is not bundled`);
  return parseTemplate(hit[1]);
}

export function loadAllTemplates(): Record<TemplateId, Template> {
  return {
    recreate: loadTemplate('recreate'),
    cinematic: loadTemplate('cinematic'),
    'calm-hour': loadTemplate('calm-hour'),
    blank: loadTemplate('blank'),
  };
}
