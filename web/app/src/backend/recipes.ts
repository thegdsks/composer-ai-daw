// Templates and recipes (D28): types and mappers shared by both backends. A recipe reads the
// project and writes ordinary commands; planning one changes nothing.
import type { Command, Id } from '../model';

export interface RecipeParam {
  name: string;
  type: string;
  choices?: string[];
  default?: unknown;
  min?: number;
  max?: number;
  description?: string;
}

export interface RecipeInfo {
  name: string;
  title: string;
  summary: string;
  scope: string;
  params: RecipeParam[];
}

export interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  tempo: number;
  key: string;
  commandCount: number;
  loop: { start: number; end: number };
}

export interface RecipeScope {
  track?: Id;
  region?: Id;
}

export interface RecipeRequest {
  recipe: string;
  params?: Record<string, unknown>;
  scope?: RecipeScope;
  seed?: number;
}

/** A recipe's command list and what it would do, with nothing applied. */
export interface RecipePlan {
  recipe: string;
  label: string;
  commandCount: number;
  changeCount: number;
  wouldChange: boolean;
  applies: boolean;
  stateHash: string;
  stateHashAfter: string;
  commands: Command[];
}

export interface RawTemplateInfo {
  id: string;
  name: string;
  description: string;
  tempo: number;
  key: string;
  command_count: number;
  loop: { start: number; end: number };
}

export interface RawRecipePlan {
  recipe: string;
  label: string;
  command_count: number;
  change_count: number;
  would_change: boolean;
  applies: boolean;
  state_hash: string;
  state_hash_after: string;
  commands: Command[];
}

export function toRawRecipe(r: RecipeRequest, dryRun = false): object {
  return {
    recipe: r.recipe,
    ...(r.params === undefined ? {} : { params: r.params }),
    ...(r.scope === undefined ? {} : { scope: r.scope }),
    ...(r.seed === undefined ? {} : { seed: r.seed }),
    ...(dryRun ? { dry_run: true } : {}),
  };
}

export function toTemplates(r: { templates: RawTemplateInfo[] }): TemplateInfo[] {
  return r.templates.map((t) => ({
    id: t.id,
    name: t.name,
    description: t.description,
    tempo: t.tempo,
    key: t.key,
    commandCount: t.command_count,
    loop: { ...t.loop },
  }));
}

export function toRecipes(r: { recipes: RecipeInfo[] }): RecipeInfo[] {
  return r.recipes;
}

export function toRecipePlan(r: RawRecipePlan): RecipePlan {
  return {
    recipe: r.recipe,
    label: r.label,
    commandCount: r.command_count,
    changeCount: r.change_count,
    wouldChange: r.would_change,
    applies: r.applies,
    stateHash: r.state_hash,
    stateHashAfter: r.state_hash_after,
    commands: r.commands,
  };
}
