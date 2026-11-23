import type { ReactNode } from 'react';
import { btn, btnPrimary } from './classes';

export interface EmptyAction {
  label: string;
  icon?: ReactNode;
  onClick(): void;
}

/** An empty area never leaves a beginner stuck: it names what is missing and offers two next steps. */
export function EmptyState(props: {
  title: string;
  hint?: string;
  primary: EmptyAction;
  secondary?: EmptyAction;
  /** Marks the first button as the screen's one main action. Leave off when the top bar already has one. */
  emphasize?: boolean;
}) {
  const { primary, secondary } = props;
  return (
    <div className="flex flex-col items-start gap-2 p-4" data-testid="empty-state">
      <p className="m-0 text-sm font-semibold text-ink">{props.title}</p>
      {props.hint && <p className="m-0 text-[13px] text-ink-dim">{props.hint}</p>}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          className={props.emphasize ? btnPrimary : btn}
          data-primary={props.emphasize ? 'true' : undefined}
          onClick={primary.onClick}
        >
          {primary.icon}
          {primary.label}
        </button>
        {secondary && (
          <button type="button" className={btn} onClick={secondary.onClick}>
            {secondary.icon}
            {secondary.label}
          </button>
        )}
      </div>
    </div>
  );
}
