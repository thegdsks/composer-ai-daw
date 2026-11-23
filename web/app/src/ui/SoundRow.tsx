import { InstrumentGlyph } from '../icons/instruments';
import type { SoundEntry } from '../backend/types';
import { attributionOf, licenceLine, type CategoryLabel } from '../sounds/catalogue';
import { ChevronIcon } from '../icons/library';
import { DOT as CATEGORY_DOT } from './SoundBrowser';
import { beginDrag, endDrag } from './dnd';

// Row heights must equal config.libraryRowHeight (h-16, 64 px) and libraryHeaderHeight (h-8, 32 px).

export function GroupRow(props: {
  id: string;
  category: CategoryLabel;
  count: number;
  collapsed: boolean;
  active: boolean;
  onToggle(): void;
}) {
  return (
    <div
      role="treeitem"
      id={props.id}
      aria-level={1}
      aria-expanded={!props.collapsed}
      aria-label={`${props.category}, ${props.count} sounds`}
      data-testid="library-group"
      data-active={props.active}
      className="flex h-8 cursor-pointer items-center gap-1.5 px-2 text-xs font-semibold uppercase tracking-wide text-ink-dim hover:text-ink data-[active=true]:bg-panel-2"
      onClick={props.onToggle}
    >
      <ChevronIcon size={14} className={props.collapsed ? '' : 'rotate-90'} />
      <span aria-hidden="true" className={`size-2 rounded-full ${CATEGORY_DOT[props.category]}`} />
      <InstrumentGlyph of={props.category} size={14} className="shrink-0" />
      <span>{props.category}</span>
      <span className="ml-auto font-normal tabular-nums">{props.count}</span>
    </div>
  );
}

export function SoundRow(props: { id: string; entry: SoundEntry; active: boolean; inUse: boolean; onPick(): void; onUse(): void }) {
  const { entry: e } = props;
  return (
    <div
      role="treeitem"
      id={props.id}
      aria-level={2}
      aria-selected={props.active}
      aria-label={`${e.name}${props.inUse ? ', in use' : ''}`}
      data-testid="library-sound"
      data-sound={e.id}
      data-active={props.active}
      title={`${e.instruments}. ${licenceLine(e)}`}
      className="flex h-16 cursor-pointer flex-col justify-center gap-0.5 border-l-2 border-transparent py-1 pl-6 pr-2 hover:bg-panel-2 data-[active=true]:border-accent data-[active=true]:bg-panel-2"
      draggable
      onDragStart={(ev) => beginDrag({ kind: 'sound', soundId: e.id, name: e.name }, ev.dataTransfer)}
      onDragEnd={endDrag}
      onClick={props.onPick}
      onDoubleClick={props.onUse}
    >
      <div className="flex items-center gap-2">
        <InstrumentGlyph of={e} size={16} data-testid="instrument-icon" className="shrink-0 text-ink-dim" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{e.name}</span>
        {props.inUse && <span className="shrink-0 rounded-full border border-accent px-1.5 text-[11px] font-medium text-ink">In use</span>}
      </div>
      <p className="m-0 truncate text-xs text-ink-dim">{e.instruments}</p>
      <p className="m-0 truncate text-[11px] text-ink-dim">
        Licence: {e.kind === 'builtin' ? `${e.licence}, made by Composer AI DAW` : e.licence}
      </p>
    </div>
  );
}

export function PackItem(props: { id: string; entry: SoundEntry; reason: string }) {
  return (
    <div
      role="treeitem"
      id={props.id}
      aria-level={1}
      aria-disabled="true"
      aria-label={`${props.entry.name}, not ready yet. ${props.reason}`}
      data-testid="library-pack"
      className="flex h-16 flex-col justify-center gap-0.5 px-3 py-1 opacity-70"
    >
      <span className="truncate text-sm text-ink-dim">{props.entry.name}</span>
      <span className="truncate text-xs text-ink-dim">{props.reason}</span>
      <span className="truncate text-[11px] text-ink-dim">Licence: {props.entry.licence}</span>
    </div>
  );
}

/** Full licence and credit for the sound in focus, shown in the footer. */
export function LicenceDetail(props: { entry: SoundEntry }) {
  const credit = attributionOf(props.entry);
  return (
    <div className="flex flex-col gap-0.5 text-xs text-ink-dim" data-testid="library-licence">
      <span>Licence: {props.entry.licence}</span>
      <span>{credit ?? 'No credit needed.'}</span>
    </div>
  );
}
