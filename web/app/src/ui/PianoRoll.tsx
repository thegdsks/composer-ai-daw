import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { config, gridChoices } from '../config';
import { getPrefs, setPrefs, subscribePrefs } from '../prefs';
import { MinusIcon, PlusIcon, TrashIcon } from '../icons';
import { keyAt, keyName } from '../roll/keyScale';
import { RollController, type RollMode, type RollView } from '../roll/RollController';
import { autoGridTicks, selectionText } from '../roll/selection';
import { findRegion, sortedNotes } from '../state/store';
import { meterSegments, ticksPerBar } from '../time';
import { btn, btnPrimary, btnToggle, compact, field, iconBtn } from './classes';
import { useSession, useSnapshot } from './hooks';
import { ChordStrip, ChordStripToggle } from './ChordStrip';
import { NoteList } from './NoteList';
import { PaletteProbe, type Palette } from './palette';
import { VelocityControls } from './VelocityControls';

export interface RollApi {
  deleteSelection(): Promise<void>;
  selectAll(): void;
  /** Quantize the selection (every note when none); false when it could not run. */
  quantize(): Promise<boolean>;
}

const AUTO_ID = 'auto';

export function PianoRoll({
  onApi,
  preview,
}: {
  onApi(api: RollApi | null): void;
  preview(pitch: number, on: boolean): void;
}) {
  const session = useSession();
  const { store, audio, actions } = session;
  const snap = useSnapshot();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const paletteRef = useRef<Palette | null>(null);
  const ctrlRef = useRef<RollController | null>(null);
  const gridRef = useRef(0);
  const sizes = useMemo(() => gridChoices(), []);
  const [tool, setTool] = useState<RollMode['tool']>('draw');
  const [ramp, setRamp] = useState(false);
  const [strength, setStrength] = useState<number>(config.quantizeStrengths[0] ?? 100);
  const [listView, setListView] = useState(false);
  // The bulk edit row costs roll height on a small screen, so it starts folded away.
  const [tools, setTools] = useState(false);
  const modeRef = useRef<RollMode>({ tool: 'draw', ramp: false, strength: 100 });
  const prefs = useSyncExternalStore(subscribePrefs, getPrefs, getPrefs);
  const laneRef = useRef(prefs.velocityLane);
  const confirmRef = useRef<HTMLDialogElement>(null);
  const [pendingDelete, setPendingDelete] = useState(0);
  const [gridId, setGridId] = useState(
    gridChoices().some((c) => c.id === `1/${config.defaultGridDenominator}`) ? `1/${config.defaultGridDenominator}` : 'off',
  );
  const [view, setView] = useState<RollView | null>(null);
  const previewRef = useRef(preview);
  const [stayInKey, setStayInKey] = useState(true);
  const stayRef = useRef(true);
  // Auto follows the zoom: the finest straight note size whose cell stays wide enough.
  const straight = useMemo(() => sizes.filter((c) => !c.id.endsWith('t')), [sizes]);
  const autoTicks = autoGridTicks(view?.pxPerTick ?? config.zoomDefault / config.ticksPerQuarter, straight);
  const autoName = straight.find((c) => c.ticks === autoTicks)?.label.toLowerCase() ?? 'notes';
  const choices = useMemo(
    () => [
      { id: AUTO_ID, label: `Auto: ${autoName}`, ticks: autoTicks },
      { id: 'off', label: 'Off', ticks: config.gridOffTicks },
      ...sizes,
    ],
    [sizes, autoName, autoTicks],
  );
  const grid = choices.find((c) => c.id === gridId) ?? choices[1];
  const gridTicks = grid?.ticks ?? config.ticksPerQuarter / 4;
  // Controllers read these through refs so a new snap or handler needs no rebuild.
  useEffect(() => {
    previewRef.current = preview;
    gridRef.current = gridTicks;
    stayRef.current = stayInKey;
    modeRef.current = { tool, ramp, strength };
    laneRef.current = prefs.velocityLane;
    ctrlRef.current?.invalidate();
  }, [preview, gridTicks, stayInKey, tool, ramp, strength, prefs.velocityLane]);

  const found = findRegion(snap.project, snap.selectedRegion);
  const sig = meterSegments(snap.project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 };
  const bar = ticksPerBar(sig);
  const firstBar = found ? Math.floor(found.region.start / bar) + 1 : 0;
  const lastBar = found ? Math.max(firstBar, Math.ceil((found.region.start + found.region.length) / bar)) : 0;
  const barsText = firstBar === lastBar ? `bar ${firstBar}` : `bars ${firstBar} to ${lastBar}`;
  const key = keyAt(snap.project.tempo_map, found?.region.start ?? 0);
  const selectedCount = snap.selectedNotes.size;
  const openRegion = found?.region ?? null;
  const selectedNotes = openRegion ? sortedNotes(openRegion).filter((n) => snap.selectedNotes.has(n.id)) : [];
  const selectedText = selectionText(selectedNotes);

  const onPalette = useCallback((p: Palette) => {
    paletteRef.current = p;
    ctrlRef.current?.invalidate();
  }, []);

  // Big deletes ask first; small ones go straight through (one undo step either way).
  const requestDelete = useCallback(async (): Promise<void> => {
    const count = store.getSnapshot().selectedNotes.size;
    if (count > config.confirmDeleteNotes) {
      setPendingDelete(count);
      return;
    }
    await ctrlRef.current?.deleteSelection();
  }, [store]);

  useEffect(() => {
    const d = confirmRef.current;
    if (!d) return;
    if (pendingDelete > 0 && !d.open) d.showModal();
    if (pendingDelete === 0 && d.open) d.close();
  }, [pendingDelete]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctrl = new RollController({
      canvas,
      store,
      audio,
      actions,
      getPalette: () => paletteRef.current,
      getGrid: () => gridRef.current,
      preview: (p, on) => previewRef.current(p, on),
      getStayInKey: () => stayRef.current,
      getMode: () => modeRef.current,
      getLane: () => laneRef.current,
      onView: () => {
        const c = ctrlRef.current;
        if (c) setView(c.getView());
      },
    });
    ctrlRef.current = ctrl;
    onApi({ deleteSelection: requestDelete, selectAll: () => ctrl.selectAll(), quantize: () => ctrl.quantizeSelection() });
    setView(ctrl.getView());
    return () => {
      ctrl.destroy();
      ctrlRef.current = null;
      onApi(null);
    };
  }, [store, audio, actions, onApi, requestDelete]);

  return (
    <section aria-label="Piano roll" className="flex min-h-0 flex-1 flex-col bg-panel">
      <PaletteProbe onPalette={onPalette} />
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-3 py-1">
        <h2 className="min-w-0 truncate text-sm font-semibold">
          {found ? (
            <>
              <span className="sr-only">Editing: </span>
              {`${found.track.name}, ${barsText}`}
            </>
          ) : (
            'Piano roll'
          )}
        </h2>
        <label className="flex items-center gap-1.5 text-sm text-ink-dim">
          Grid
          <select
            className={compact(field)}
            value={gridId}
            onChange={(e) => setGridId(e.target.value)}
            aria-label="Grid: notes snap to this size"
          >
            {choices.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className={compact(btn)}
          aria-pressed={stayInKey && key !== null}
          disabled={key === null}
          data-testid="stay-in-key"
          title={key ? `Notes snap to the nearest note of ${keyName(key)}` : 'Pick a key in the top bar to use this'}
          onClick={() => setStayInKey((v) => !v)}
        >
          {stayInKey && key ? 'Stay in key: on' : 'Stay in key: off'}
        </button>
        <div className="flex items-center" role="group" aria-label="Zoom">
          <button type="button" className={iconBtn} aria-label="Zoom out (minus)" title="Zoom out" onClick={() => ctrlRef.current?.zoomBy(1 / config.zoomStep)}>
            <MinusIcon size={18} />
          </button>
          <button type="button" className={iconBtn} aria-label="Zoom in (plus)" title="Zoom in" onClick={() => ctrlRef.current?.zoomBy(config.zoomStep)}>
            <PlusIcon size={18} />
          </button>
          <button type="button" className={compact(btn)} onClick={() => ctrlRef.current?.fit()} title="Frame the clip (Home)">
            Fit
          </button>
        </div>
        <button
          type="button"
          className={iconBtn}
          disabled={selectedCount === 0}
          aria-label="Delete notes (Delete)"
          onClick={() => void requestDelete()}
          title="Delete selected notes (Delete)"
        >
          <TrashIcon size={16} />
        </button>
        <ChordStripToggle />
        <button type="button" className={compact(btnToggle(tools))} aria-pressed={tools} aria-expanded={tools} data-testid="edit-tools" title="Select, tidy timing, transpose and loudness for many notes" onClick={() => setTools((v) => !v)}>
          Edit tools
        </button>
        <button
          type="button"
          className={compact(btnToggle(view?.laneOpen ?? true))}
          aria-pressed={view?.laneOpen ?? true}
          data-testid="velocity-lane-toggle"
          title="Show or hide the loudness lane under the notes"
          onClick={() => setPrefs({ velocityLane: view?.laneOpen === false ? 'open' : 'closed' })}
        >
          Loudness
        </button>
        <p className="ml-auto min-w-0 truncate text-xs text-ink-dim max-[1500px]:sr-only" role="status" data-testid="roll-status">
          {found
            ? `${selectedText}. ${tool === 'select' ? 'Drag on the grid to select notes.' : 'Click and drag on the grid to draw a note. Shift drag selects.'}`
            : 'Select a clip in the arrangement, or double click a lane to add one.'}
        </p>
      </div>
      {found && <ChordStrip region={found.region} gridTicks={gridTicks} />}
      {found && (tools || listView) && (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-3 py-1" role="toolbar" aria-label="Edit many notes">
          <div className="flex items-center gap-1" role="group" aria-label="Pointer tool">
            <button type="button" className={compact(btnToggle(tool === 'draw'))} aria-pressed={tool === 'draw'} onClick={() => setTool('draw')} title="Drag on the grid to draw notes">
              Draw
            </button>
            <button type="button" className={compact(btnToggle(tool === 'select'))} aria-pressed={tool === 'select'} data-testid="tool-select" onClick={() => setTool('select')} title="Drag on the grid to select notes (Shift drag does this in Draw)">
              Select
            </button>
            <button type="button" className={compact(btn)} onClick={() => ctrlRef.current?.selectAll()} title="Select every note of this clip (Ctrl A)">
              Select all
            </button>
            <button type="button" className={compact(btn)} onClick={() => void actions.deleteRegion(found.region.id)} title="Delete this clip and its notes">
              Delete clip
            </button>
          </div>
          <div className="flex items-center gap-1" role="group" aria-label="Tidy timing">
            <button
              type="button"
              className={compact(btn)}
              data-testid="quantize"
              title={`Pull ${selectedCount > 0 ? 'the selected' : 'all'} notes onto the grid (Q)`}
              onClick={() => void ctrlRef.current?.quantizeSelection()}
              onPointerEnter={() => ctrlRef.current?.setQuantizeGhost(true)}
              onPointerLeave={() => ctrlRef.current?.setQuantizeGhost(false)}
              onFocus={() => ctrlRef.current?.setQuantizeGhost(true)}
              onBlur={() => ctrlRef.current?.setQuantizeGhost(false)}
            >
              Tidy timing
            </button>
            <label className="flex items-center gap-1 text-sm text-ink-dim">
              Strength
              <select className={compact(field)} value={strength} aria-label="Tidy strength" onChange={(e) => setStrength(Number(e.target.value))}>
                {config.quantizeStrengths.map((v) => (
                  <option key={v} value={v}>
                    {v} percent
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="flex items-center gap-1" role="group" aria-label="Transpose">
            <button type="button" className={compact(btn)} onClick={() => void ctrlRef.current?.transposeSelection(-1)} title="Down one semitone">
              Down
            </button>
            <button type="button" className={compact(btn)} onClick={() => void ctrlRef.current?.transposeSelection(1)} title="Up one semitone">
              Up
            </button>
            <button type="button" className={compact(btn)} onClick={() => void ctrlRef.current?.transposeSelection(12)} title="Up one octave">
              Octave up
            </button>
          </div>
          <VelocityControls region={found.region.id} selected={selectedNotes} ramp={ramp} onRamp={setRamp} />
          <button
            type="button"
            className={compact(btnToggle(listView))}
            aria-pressed={listView}
            data-testid="note-list-toggle"
            title="Show the notes as a list you can edit with the keyboard"
            onClick={() => setListView((v) => !v)}
          >
            Note list
          </button>
        </div>
      )}
      {found && listView && <NoteList region={found.region} trackName={found.track.name} />}
      <dialog
        ref={confirmRef}
        aria-labelledby="confirm-delete-title"
        data-testid="confirm-delete"
        className="m-auto w-96 max-w-[92vw] overscroll-contain rounded-lg border border-line bg-panel p-4 text-ink backdrop:overflow-hidden backdrop:overscroll-contain backdrop:bg-black/50"
        onClose={() => setPendingDelete(0)}
      >
        <div className="flex flex-col gap-3">
          <h2 id="confirm-delete-title" className="text-base font-semibold">
            Delete {pendingDelete} notes?
          </h2>
          <p className="m-0 text-sm text-ink-dim">You can undo this with Ctrl or Cmd Z.</p>
          <div className="flex justify-end gap-2">
            <button type="button" autoFocus className={btn} onClick={() => setPendingDelete(0)}>
              Keep notes
            </button>
            <button
              type="button"
              className={btnPrimary}
              onClick={() => {
                setPendingDelete(0);
                void ctrlRef.current?.deleteSelection();
              }}
            >
              Delete notes
            </button>
          </div>
        </div>
      </dialog>
      <div className={listView ? 'hidden' : 'flex min-h-0 flex-1 flex-col'}>
      <div className="flex min-h-0 flex-1">
        <div className="relative min-w-0 flex-1">
          <canvas
            id="piano-roll"
            ref={canvasRef}
            tabIndex={0}
            role="application"
            aria-label="Piano roll. Drag on the grid to draw notes, drag a note to move it, drag its right edge to resize it. Enter adds a note, arrow keys move selected notes, Delete removes them."
            className="block h-full w-full touch-none"
          />
        </div>
        <input
          type="range"
          aria-label="Scroll the piano roll up and down"
          data-testid="roll-vscroll"
          className="w-5 shrink-0 accent-accent [writing-mode:vertical-lr]"
          min={0}
          max={view?.maxScrollY ?? 0}
          step={view?.rowHeight ?? 1}
          value={view?.scrollY ?? 0}
          onChange={(e) => ctrlRef.current?.setScrollY(Number(e.target.value))}
        />
      </div>
      <input
        type="range"
        aria-label="Scroll the piano roll in time"
        className="h-5 w-full accent-accent"
        min={view?.minTick ?? 0}
        max={Math.max(view?.minTick ?? 0, view?.maxTick ?? 0)}
        step={config.ticksPerQuarter}
        value={view?.scrollTick ?? 0}
        onChange={(e) => ctrlRef.current?.setScrollTick(Number(e.target.value))}
      />
      </div>
    </section>
  );
}
