import { useState, type DragEvent, type KeyboardEvent } from 'react';
import { ChevronDownIcon, ChevronUpIcon, GripIcon, PowerIcon, XIcon } from '../icons';
import { DeviceIcon } from '../icons/devices';
import type { Effect } from '../model';
import { beginDrag, endDrag, getDrag } from './dnd';
import { StripSlider } from './mixer/StripSlider';
import { clampParam, deviceOf, paramText, paramValue, type ParamUi } from '../state/effectsModel';
import type { Actions } from '../state/actions';

const iconButton =
  'inline-flex size-8 items-center justify-center rounded text-ink-dim hover:bg-panel-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent aria-disabled:cursor-not-allowed aria-disabled:opacity-40 aria-disabled:hover:bg-transparent';
const slider = 'h-6 w-full min-w-0 accent-accent';

function ParamControl(props: { effect: Effect; spec: ParamUi; actions: Actions }) {
  const { effect, spec, actions } = props;
  const value = paramValue(effect)(spec.name);
  const label = `${deviceOf(effect.kind).name} ${spec.label}`;
  const show = (v: number): string => paramText(effect.kind, spec.name, v);
  if (spec.toggle) {
    const on = value !== 0;
    return (
      <div className="flex items-center justify-between gap-2 text-xs text-ink-dim">
        <span>{spec.label}</span>
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label={label}
          data-testid={`effect-param-${spec.name}`}
          className="h-6 min-w-12 rounded border border-line px-2 text-xs text-ink hover:bg-panel-2 aria-checked:bg-ink aria-checked:text-surface"
          onClick={() => void actions.setEffectParam(effect, spec.name, on ? 0 : 1)}
        >
          {show(value)}
        </button>
      </div>
    );
  }
  return (
    <div className="flex flex-col text-xs text-ink-dim">
      <span className="flex justify-between gap-2">
        <span>{spec.label}</span>
        <span className="tabular-nums text-ink">{show(value)}</span>
      </span>
      <StripSlider
        label={label}
        testId={`effect-param-${spec.name}`}
        className={slider}
        value={value}
        min={spec.min}
        max={spec.max}
        step={spec.step}
        neutral={spec.default}
        text={show}
        same={(d) => clampParam(effect.kind, spec.name, d) === value}
        onCommit={(v) => actions.setEffectParam(effect, spec.name, v)}
      />
    </div>
  );
}

/** The control that keeps focus after a card moves (test id). */
export type EffectFocus = 'effect-up' | 'effect-down' | 'effect-grip';

/** One device of a chain: power, name, move and remove buttons, and a generated panel of its parameters. */
export function EffectCard(props: {
  effect: Effect;
  index: number;
  count: number;
  actions: Actions;
  /** Moves this card to a chain position; the caller owns focus after the move. */
  onMove(effectId: string, to: number, focus: EffectFocus): void;
  /** Index a card dropped here would take, or null when it would not move. */
  dropTarget(draggedId: string, onto: string): number | null;
}) {
  const { effect, index, count, actions } = props;
  const device = deviceOf(effect.kind);
  const bypassed = effect.bypass === true;
  const [over, setOver] = useState(false);

  const canUp = index > 0;
  const canDown = index < count - 1;
  const up = (focus: EffectFocus): void => {
    if (canUp) props.onMove(effect.id, index - 1, focus);
  };
  const down = (focus: EffectFocus): void => {
    if (canDown) props.onMove(effect.id, index + 1, focus);
  };

  const onGripKey = (e: KeyboardEvent): void => {
    if (!e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return;
    e.preventDefault();
    if (e.key === 'ArrowUp') up('effect-grip');
    else down('effect-grip');
  };
  const onDragOver = (e: DragEvent): void => {
    const d = getDrag();
    if (d?.kind !== 'effect' || props.dropTarget(d.effectId, effect.id) === null) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    setOver(true);
  };
  const onDrop = (e: DragEvent): void => {
    const d = getDrag();
    setOver(false);
    if (d?.kind !== 'effect') return;
    const to = props.dropTarget(d.effectId, effect.id);
    if (to === null) return;
    e.preventDefault();
    endDrag();
    props.onMove(d.effectId, to, 'effect-grip');
  };

  return (
    <li
      data-testid="effect-card"
      data-effect={effect.id}
      data-kind={effect.kind}
      data-bypassed={bypassed}
      aria-label={`${device.name}, ${bypassed ? 'bypassed' : 'on'}, ${index + 1} of ${count}`}
      className={`flex w-72 shrink-0 flex-col gap-1 rounded-md border bg-surface p-2 ${over ? 'border-accent' : 'border-line'}`}
      onDragOver={onDragOver}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
    >
      <div className="flex items-center gap-1">
        <button
          type="button"
          draggable
          aria-label={`Reorder ${device.name}. Alt and arrow keys move it.`}
          title="Drag to reorder, or Alt and Up or Down arrow"
          data-testid="effect-grip"
          className={`${iconButton} cursor-grab`}
          onKeyDown={onGripKey}
          onDragStart={(e) => {
            beginDrag({ kind: 'effect', effectId: effect.id, name: device.name }, e.dataTransfer);
            const card = e.currentTarget.closest('li');
            if (card) e.dataTransfer.setDragImage(card, 8, 8);
          }}
          onDragEnd={() => endDrag()}
        >
          <GripIcon size={16} />
        </button>
        <span className={`flex min-w-0 flex-1 items-center gap-1.5 text-sm font-medium ${bypassed ? 'text-ink-dim' : 'text-ink'}`}>
          <DeviceIcon kind={effect.kind} />
          <span className="leading-tight">{device.name}</span>
        </span>
        <button
          type="button"
          role="switch"
          aria-checked={!bypassed}
          aria-label={`${device.name} on`}
          title={bypassed ? 'Bypassed, turn on' : 'On, bypass'}
          data-testid="effect-bypass"
          className={`${iconButton} aria-checked:text-ink`}
          onClick={() => void actions.setEffectBypass(effect.id, !bypassed)}
        >
          <PowerIcon size={16} />
        </button>
        <button
          type="button"
          aria-label={`Move ${device.name} earlier`}
          aria-disabled={!canUp}
          data-testid="effect-up"
          className={iconButton}
          onClick={() => up('effect-up')}
        >
          <ChevronUpIcon size={16} />
        </button>
        <button
          type="button"
          aria-label={`Move ${device.name} later`}
          aria-disabled={!canDown}
          data-testid="effect-down"
          className={iconButton}
          onClick={() => down('effect-down')}
        >
          <ChevronDownIcon size={16} />
        </button>
        <button
          type="button"
          aria-label={`Remove ${device.name}`}
          data-testid="effect-remove"
          className={iconButton}
          onClick={() => void actions.removeEffect(effect.id)}
        >
          <XIcon size={16} />
        </button>
      </div>
      <div className={`flex flex-col gap-1 ${bypassed ? 'opacity-60' : ''}`}>
        {device.params.map((spec) => (
          <ParamControl key={spec.name} effect={effect} spec={spec} actions={actions} />
        ))}
      </div>
    </li>
  );
}
