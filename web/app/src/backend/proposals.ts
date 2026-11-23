// Proposals channel (D27): types and mappers shared by both backends. A proposal is a
// labelled command list from an AI client that changes nothing until a person accepts it.
import type { Command } from '../model';

export type ProposalStatus = 'pending' | 'accepted' | 'rejected';

export interface ProposalPost {
  label: string;
  commands: Command[];
  note?: string;
  author?: string;
  dryRun?: boolean;
}

export interface ProposalPosted {
  /** Absent on a dry run. */
  id: string | null;
  changeCount: number;
  stateHash: string;
  stateHashAfter: string;
  wouldChange: boolean;
}

export interface Proposal {
  id: string;
  label: string;
  note: string | null;
  author: string | null;
  status: ProposalStatus;
  commandCount: number;
  commands: Command[];
  baseHash: string;
  /** The project moved since the proposal was posted. */
  stale: boolean;
  /** Pending only: whether the commands still apply to the project now. */
  applies?: boolean;
  changeCount?: number;
}

export interface ProposalList {
  proposals: Proposal[];
  pending: number;
  stateHash: string;
}

export type ProposalFilter = ProposalStatus | 'all';

export interface RawProposalPosted {
  id?: string;
  change_count: number;
  state_hash: string;
  state_hash_after: string;
  would_change: boolean;
}

export interface RawProposal {
  id: string;
  label: string;
  note: string | null;
  author: string | null;
  status: ProposalStatus;
  command_count: number;
  commands: Command[];
  base_hash: string;
  stale: boolean;
  applies?: boolean;
  change_count?: number;
}

export interface RawProposalList {
  proposals: RawProposal[];
  pending: number;
  state_hash: string;
}

export function toRawPost(p: ProposalPost): object {
  return {
    label: p.label,
    commands: p.commands,
    ...(p.note === undefined ? {} : { note: p.note }),
    ...(p.author === undefined ? {} : { author: p.author }),
    ...(p.dryRun ? { dry_run: true } : {}),
  };
}

export function toProposalPosted(r: RawProposalPosted): ProposalPosted {
  return {
    id: r.id ?? null,
    changeCount: r.change_count,
    stateHash: r.state_hash,
    stateHashAfter: r.state_hash_after,
    wouldChange: r.would_change,
  };
}

export function toProposalList(r: RawProposalList): ProposalList {
  return {
    proposals: r.proposals.map((p) => ({
      id: p.id,
      label: p.label,
      note: p.note,
      author: p.author,
      status: p.status,
      commandCount: p.command_count,
      commands: p.commands,
      baseHash: p.base_hash,
      stale: p.stale,
      ...(p.applies === undefined ? {} : { applies: p.applies }),
      ...(p.change_count === undefined ? {} : { changeCount: p.change_count }),
    })),
    pending: r.pending,
    stateHash: r.state_hash,
  };
}
