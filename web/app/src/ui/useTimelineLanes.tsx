import { useCallback, useEffect, useRef, useState, useSyncExternalStore, type ReactNode, type RefObject } from 'react';
import { config } from '../config';
import type { Tick } from '../model';
import type { ArrangementController } from '../roll/ArrangementController';
import { keyAt, ROOT_NAMES, SCALES } from '../roll/keyScale';
import { TimelineLanes, type LaneMenuRequest, type LaneRequest } from '../roll/TimelineLanes';
import { getLanes, LANE_IDS, LANE_LABELS, subscribeLanes, toggleLane, visibleLanes, type LaneId } from '../state/laneStore';
import {
  addMarker,
  keyChange,
  removeMarker,
  removeTempoAt,
  renameMarker,
  tempoAt,
} from '../state/timelinePlans';
import type { ScaleName } from '../model';
import { bpmAt, secondsToTicks, snapRound, ticksPerBeat, meterSegments } from '../time';
import { LanesIcon } from '../icons/timeline';
import { PlusIcon } from '../icons';
import { ContextMenu, type MenuRequest } from './ContextMenu';
import { Dialog } from './Dialog';
import { field } from './classes';
import { useSession } from './hooks';
import type { Palette } from './palette';
import type { MenuItem } from './menuModel';

// Class names are spelled out so Tailwind sees them. Keep each equal to laneHeight times its row count.
const HEIGHT: Record<number, string> = { 1: 'h-6', 2: 'h-12', 3: 'h-[4.5rem]' };

const ADD_TEXT: Record<LaneId, { label: string; title: string }> = {
  markers: { label: 'Add a marker at the playhead', title: 'Add a marker at the playhead' },
  tempo: { label: 'Add a tempo change at the playhead', title: 'Add a tempo change at the playhead' },
  key: { label: 'Add a key change at the playhead', title: 'Add a key change at the playhead' },
};

interface Edit {
  lane: LaneId;
  at: Tick;
  /** True when a point already sits at `at`. */
  existing: boolean;
}

const iconTool =
  'inline-flex size-5 items-center justify-center rounded text-ink hover:bg-panel-2 motion-safe:transition-colors';

/**
 * The marker, tempo and key lanes. The panel puts `labels` above the track list, `strip` above the
 * arrangement canvas, `toolbar` in the view toolbar and `overlays` anywhere. All edits go through
 * the store as commands; the lanes only turn gestures into requests.
 */
export function useTimelineLanes(ctrl: RefObject<ArrangementController | null>, palette: RefObject<Palette | null>) {
  const session = useSession();
  const { store, audio, transport } = session;
  const set = useSyncExternalStore(subscribeLanes, getLanes);
  const lanes = visibleLanes(set);
  const lanesRef = useRef<readonly LaneId[]>(lanes);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewRef = useRef<TimelineLanes | null>(null);
  const [edit, setEdit] = useState<Edit | null>(null);
  const [text, setText] = useState('');
  const [root, setRoot] = useState(0);
  const [scale, setScale] = useState<ScaleName>('major');
  const [menu, setMenu] = useState<MenuRequest | null>(null);

  useEffect(() => {
    lanesRef.current = lanes;
    viewRef.current?.invalidate();
  }, [lanes]);

  const startEdit = useCallback(
    (lane: LaneId, at: Tick, existing: boolean): void => {
      const p = store.getSnapshot().project;
      if (lane === 'markers') setText(p.markers?.[String(at)] ?? '');
      else if (lane === 'tempo') setText(String(Math.round(bpmAt(p.tempo_map, at) * 100) / 100));
      else {
        const k = keyAt(p.tempo_map, at);
        setRoot(k?.root ?? 0);
        setScale(k?.scale ?? 'major');
      }
      setEdit({ lane, at, existing });
    },
    [store],
  );

  const onEdit = useCallback(
    (req: LaneRequest): void => {
      if (req.lane === 'markers' && req.point === null) {
        const cmd = addMarker(store.getSnapshot().project, req.at);
        if (cmd) void store.apply(cmd);
        return;
      }
      startEdit(req.lane, req.point ?? req.at, req.point !== null);
    },
    [store, startEdit],
  );

  const onMenu = useCallback(
    (req: LaneMenuRequest): void => {
      const at = req.point as Tick;
      const items: MenuItem[] = [];
      if (req.lane === 'markers') {
        items.push({ id: 'go', label: 'Go to marker', run: () => void transport.seek(at) });
        items.push({ id: 'rename', label: 'Rename marker', run: () => startEdit('markers', at, true) });
        items.push({ id: 'remove', label: 'Remove marker', separated: true, danger: true, run: () => void store.apply(removeMarker(at)) });
      } else if (req.lane === 'tempo') {
        const remove = removeTempoAt(at);
        items.push({ id: 'change', label: 'Change tempo', run: () => startEdit('tempo', at, true) });
        items.push({
          id: 'remove',
          label: 'Remove tempo change',
          separated: true,
          danger: true,
          disabled: remove === null,
          reason: 'The first tempo stays',
          run: () => remove && void store.apply(remove),
        });
      } else {
        items.push({ id: 'change', label: 'Change key', run: () => startEdit('key', at, true) });
        items.push({ id: 'remove', label: 'Remove key change', separated: true, danger: true, run: () => void store.apply({ command: 'remove_key', tick: at }) });
      }
      setMenu({ label: LANE_LABELS[req.lane], items, x: req.x, y: req.y });
    },
    [store, transport, startEdit],
  );

  const callbacks = useRef({ onEdit, onMenu });
  useEffect(() => {
    callbacks.current = { onEdit, onMenu };
  }, [onEdit, onMenu]);

  const active = lanes.length > 0;
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const view = new TimelineLanes({
      canvas,
      store,
      audio,
      view: () => ctrl.current?.getView() ?? { scrollTick: 0, pxPerTick: config.arrangementZoomDefault / config.ticksPerQuarter },
      lanes: () => lanesRef.current,
      getPalette: () => palette.current,
      onEdit: (r) => callbacks.current.onEdit(r),
      onMenu: (r) => callbacks.current.onMenu(r),
      seek: (t) => void transport.seek(t),
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [active, store, audio, transport, ctrl, palette]);

  /** The playhead rounded to the nearest beat, where the label buttons add things. */
  const playheadTick = (): Tick => {
    const s = audio.status;
    const map = store.getSnapshot().project.tempo_map;
    const raw = s.sampleRate > 0 ? secondsToTicks(map, s.positionFrames / s.sampleRate) : 0;
    const sig = meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 };
    return Math.max(0, snapRound(raw, ticksPerBeat(sig)));
  };

  const addAtPlayhead = (lane: LaneId): void => {
    const at = playheadTick();
    if (lane === 'markers') {
      const cmd = addMarker(store.getSnapshot().project, at);
      if (cmd) void store.apply(cmd);
      else startEdit('markers', at, true);
    } else {
      const p = store.getSnapshot().project;
      const exists = lane === 'tempo' ? p.tempo_map.tempos[String(at)] !== undefined : p.tempo_map.keys?.[String(at)] !== undefined;
      startEdit(lane, at, exists);
    }
  };

  const commit = (): void => {
    if (!edit) return;
    const p = store.getSnapshot().project;
    if (edit.lane === 'markers') {
      const cmd = renameMarker(p, edit.at, text);
      if (cmd) void store.apply(cmd);
    } else if (edit.lane === 'tempo') {
      const cmd = tempoAt(edit.at, Number(text));
      if (cmd) void store.apply(cmd);
    } else {
      void store.apply(keyChange(edit.at, root, scale));
    }
    setEdit(null);
  };

  const labels: ReactNode =
    lanes.length === 0 ? null : (
      <div role="group" aria-label="Timeline lanes" className="flex flex-col border-b border-line">
        {lanes.map((lane) => (
          <div key={lane} data-testid={`lane-label-${lane}`} className="flex h-6 items-center justify-between border-b border-line px-2 last:border-b-0">
            <span className="text-xs font-medium text-ink-dim">{LANE_LABELS[lane]}</span>
            <button
              type="button"
              className={iconTool}
              aria-label={ADD_TEXT[lane].label}
              title={ADD_TEXT[lane].title}
              data-testid={`lane-add-${lane}`}
              onClick={() => addAtPlayhead(lane)}
            >
              <PlusIcon size={14} />
            </button>
          </div>
        ))}
      </div>
    );

  const strip: ReactNode =
    lanes.length === 0 ? null : (
      <canvas
        ref={canvasRef}
        id="timeline-lanes-canvas"
        role="img"
        aria-label={`${lanes.map((l) => LANE_LABELS[l]).join(', ')} lanes. Double click to add or edit, right click a point for more. The plus buttons at the left add at the playhead.`}
        className={`block w-full shrink-0 cursor-pointer touch-none border-b border-line ${HEIGHT[lanes.length] ?? 'h-6'}`}
      />
    );

  const toolbar: ReactNode = (
    <button
      type="button"
      className={iconTool}
      aria-label="Timeline lanes"
      aria-haspopup="menu"
      title="Show or hide the marker, tempo and key lanes"
      data-testid="lanes-menu"
      onClick={(e) => {
        const box = e.currentTarget.getBoundingClientRect();
        setMenu({
          label: 'Timeline lanes',
          x: box.left,
          y: box.bottom,
          items: LANE_IDS.map((id) => ({
            id,
            label: `${set.has(id) ? 'Hide' : 'Show'} ${LANE_LABELS[id].toLowerCase()}`,
            run: () => toggleLane(id),
          })),
        });
      }}
    >
      <LanesIcon size={14} />
    </button>
  );

  const title = edit ? dialogTitle(edit) : '';
  const overlays: ReactNode = (
    <>
      <ContextMenu request={menu} onClose={() => setMenu(null)} />
      <Dialog
        open={edit !== null}
        onClose={() => setEdit(null)}
        title={title}
        testId="lane-dialog"
        onSubmit={commit}
        primary={{ label: edit?.lane === 'markers' ? 'Rename' : 'Set', disabled: edit?.lane === 'tempo' ? !Number.isFinite(Number(text)) || text.trim() === '' : edit?.lane === 'markers' ? !text.trim() : false }}
      >
        {edit?.lane === 'markers' && (
          <label className="flex flex-col gap-1 text-sm text-ink-dim">
            Marker name
            <input data-autofocus="" className={`${field} text-ink`} value={text} autoComplete="off" onChange={(e) => setText(e.target.value)} />
          </label>
        )}
        {edit?.lane === 'tempo' && (
          <label className="flex flex-col gap-1 text-sm text-ink-dim">
            Beats per minute
            <input
              data-autofocus=""
              data-testid="lane-bpm"
              inputMode="decimal"
              className={`${field} text-ink tabular-nums`}
              value={text}
              autoComplete="off"
              onChange={(e) => setText(e.target.value)}
            />
          </label>
        )}
        {edit?.lane === 'key' && (
          <div className="flex gap-2">
            <label className="flex flex-1 flex-col gap-1 text-sm text-ink-dim">
              Key note
              <select data-autofocus="" className={`${field} text-ink`} value={root} onChange={(e) => setRoot(Number(e.target.value))}>
                {ROOT_NAMES.map((n, i) => (
                  <option key={n} value={i}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-1 flex-col gap-1 text-sm text-ink-dim">
              Scale
              <select className={`${field} text-ink`} value={scale} onChange={(e) => setScale(e.target.value as ScaleName)}>
                {SCALES.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}
      </Dialog>
    </>
  );

  return { labels, strip, toolbar, overlays };
}

function dialogTitle(e: Edit): string {
  if (e.lane === 'markers') return 'Rename marker';
  if (e.lane === 'tempo') return e.existing ? 'Change tempo' : 'Add a tempo change';
  return e.existing ? 'Change key' : 'Add a key change';
}
