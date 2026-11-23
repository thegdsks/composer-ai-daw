import { useId, useMemo, useState } from 'react';
import type { SoundEntry } from '../backend/types';
import { PlayIcon, PlusIcon } from '../icons';
import {
  CATEGORIES,
  attributionOf,
  browse,
  categoryOf,
  licenceLine,
  type Category,
  type CategoryLabel,
} from '../sounds/catalogue';
import { EmptyState } from './EmptyState';
import { btn, btnSmall, field } from './classes';
import { useSession, useSnapshot } from './hooks';

export const DOT: Record<CategoryLabel, string> = {
  Keys: 'bg-family-keys',
  Strings: 'bg-family-strings',
  Pads: 'bg-family-pads',
  Bass: 'bg-family-bass',
  Plucks: 'bg-family-plucks',
  Choir: 'bg-family-choir',
  Bells: 'bg-family-bells',
  Pulse: 'bg-family-perc',
  More: 'bg-line',
};

const chip =
  'inline-flex h-7 items-center rounded-full border px-3 text-xs font-medium motion-safe:transition-colors ';

function Card(props: { entry: SoundEntry; current: boolean; onHear(): void; onUse(): void }) {
  const { entry: e } = props;
  const cat = categoryOf(e);
  const credit = attributionOf(e);
  return (
    <li
      data-testid="sound-card"
      data-sound={e.id}
      className={`flex flex-col gap-2 rounded-md border bg-panel p-3 ${props.current ? 'border-accent' : 'border-line'}`}
    >
      <div className="flex items-center gap-2">
        <span aria-hidden="true" className={`size-2.5 shrink-0 rounded-full ${DOT[cat]}`} />
        <h3 className="m-0 min-w-0 flex-1 truncate text-sm font-semibold text-ink">{e.name}</h3>
        <span className="text-xs text-ink-dim">{cat}</span>
      </div>
      <p className="m-0 text-[13px] text-ink-dim">{e.instruments}</p>
      <p className="m-0 text-xs text-ink-dim">{licenceLine(e)}</p>
      <div className="flex items-center gap-2">
        <button type="button" className={btnSmall} aria-label={`Hear ${e.name}`} onClick={props.onHear}>
          <PlayIcon size={12} />
          Hear
        </button>
        <button
          type="button"
          className={`${btnSmall} ${props.current ? 'border-accent' : ''}`}
          aria-label={props.current ? `${e.name} is in use` : `Use ${e.name}`}
          aria-pressed={props.current}
          onClick={props.onUse}
        >
          {props.current ? 'In use' : 'Use'}
        </button>
        <details className="ml-auto text-xs text-ink-dim">
          <summary className="cursor-pointer select-none rounded-md px-1 py-0.5 hover:text-ink">About</summary>
          <div className="mt-1 flex flex-col gap-1">
            <span>Licence: {e.licence}</span>
            <span>{credit ?? 'No credit needed.'}</span>
          </div>
        </details>
      </div>
    </li>
  );
}

function PackRow(props: { entry: SoundEntry; reason: string }) {
  const id = useId();
  const { entry: e } = props;
  return (
    <li className="flex items-center gap-3 rounded-md border border-line bg-panel px-3 py-2" data-testid="sound-pack">
      <div className="min-w-0 flex-1">
        <p className="m-0 truncate text-sm text-ink-dim">{e.name}</p>
        <p id={id} className="m-0 text-xs text-ink-dim">
          {props.reason}
        </p>
      </div>
      <button type="button" className={btnSmall} disabled aria-describedby={id}>
        Use
      </button>
    </li>
  );
}

/** Sound tab: pick a voice for the selected track, with a short audition before committing. */
export function SoundBrowser(props: { onNotes(): void }) {
  const { actions, audio, sounds } = useSession();
  const snap = useSnapshot();
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<Category | null>(null);
  const searchId = useId();

  const entries = sounds.catalogue?.entries;
  const result = useMemo(() => browse(entries ?? [], query, category), [entries, query, category]);
  const track = snap.project.tracks.find((t) => t.id === snap.selectedTrack) ?? null;

  if (!track) {
    return (
      <EmptyState
        title="Add a track to give it a sound"
        hint="Sounds belong to a track. Add one, then come back and pick."
        primary={{ label: 'Add a track', icon: <PlusIcon size={16} />, onClick: () => void actions.addTrack() }}
        secondary={{ label: 'Back to Notes', onClick: props.onNotes }}
      />
    );
  }
  if (!sounds.catalogue) {
    return (
      <EmptyState
        title="The sound list is not available"
        hint={sounds.error ?? 'Reload the page, or connect to the daemon to read its catalogue.'}
        primary={{ label: 'Back to Notes', onClick: props.onNotes }}
      />
    );
  }

  const hear = (e: SoundEntry): void => void audio.preview(e.id);
  const use = (e: SoundEntry): void => {
    void actions.setSound(track.id, e.id).then((changed) => {
      if (changed) void audio.preview(e.id);
    });
  };

  return (
    <div className="flex flex-col gap-3 p-3" data-testid="sound-browser">
      <div className="flex flex-wrap items-center gap-2">
        <label htmlFor={searchId} className="sr-only">
          Search sounds
        </label>
        <input
          id={searchId}
          data-testid="sound-search"
          type="search"
          autoComplete="off"
          placeholder="Search sounds"
          className={`${field} w-56 max-w-full`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div role="group" aria-label="Sound categories" className="flex flex-wrap gap-1.5">
          <button
            type="button"
            aria-pressed={category === null}
            className={`${chip}${category === null ? 'border-accent bg-accent text-accent-ink' : 'border-line text-ink hover:bg-panel-2'}`}
            onClick={() => setCategory(null)}
          >
            All
          </button>
          {CATEGORIES.map((c) => (
            <button
              key={c}
              type="button"
              aria-pressed={category === c}
              className={`${chip}${category === c ? 'border-accent bg-accent text-accent-ink' : 'border-line text-ink hover:bg-panel-2'}`}
              onClick={() => setCategory(category === c ? null : c)}
            >
              {c}
            </button>
          ))}
        </div>
        <p className="m-0 ml-auto text-xs text-ink-dim" data-testid="sound-target">
          Sound for <span className="font-semibold text-ink">{track.name}</span>
        </p>
      </div>

      {result.readyCount === 0 && result.packs.length === 0 ? (
        <EmptyState
          title="No sound matches"
          hint="Try a shorter word, or show every category."
          primary={{
            label: 'Show all sounds',
            onClick: () => {
              setQuery('');
              setCategory(null);
            },
          }}
        />
      ) : (
        <>
          <ul
            aria-label="Sounds"
            className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-2 p-0"
          >
            {result.groups
              .flatMap((g) => g.ready)
              .map((e) => (
                <Card key={e.id} entry={e} current={track.sound === e.id} onHear={() => hear(e)} onUse={() => use(e)} />
              ))}
          </ul>
          {result.packs.length > 0 && (
            <details className="text-sm text-ink-dim" data-testid="sound-packs">
              <summary className="cursor-pointer select-none py-1 text-ink">
                Sample packs ({result.packs.length}), not ready yet
              </summary>
              <ul className="m-0 mt-2 grid list-none grid-cols-[repeat(auto-fill,minmax(18rem,1fr))] gap-2 p-0">
                {result.packs.map((p) => (
                  <PackRow key={p.entry.id} entry={p.entry} reason={p.reason} />
                ))}
              </ul>
            </details>
          )}
        </>
      )}
      {track.sound && (
        <div>
          <button type="button" className={btn} onClick={() => void actions.setSound(track.id, null)}>
            Use the basic instrument again
          </button>
        </div>
      )}
    </div>
  );
}
