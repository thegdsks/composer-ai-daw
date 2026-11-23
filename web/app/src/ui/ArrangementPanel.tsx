import { useCallback, useEffect, useMemo, useRef, useSyncExternalStore } from 'react';
import { getPrefs, subscribePrefs } from '../prefs';
import { ArrangementController } from '../roll/ArrangementController';
import { INSTRUMENT_SAW, INSTRUMENT_SINE, mixOf, type Track } from '../model';
import { config } from '../config';
import { panText, volumeText } from '../state/mix';
import { MixSlider } from './MixSlider';
import { CopyIcon, LoopIcon, MinusIcon, PlusIcon, RepeatNIcon, ScissorsIcon, TrashIcon, UploadIcon } from '../icons';
import { EmptyState } from './EmptyState';
import { btnSmall, compact, field, iconBtn } from './classes';
import { useSession, useSnapshot } from './hooks';
import { type Palette, PaletteProbe } from './palette';
import { ColorChip, Grip, MenuButton, TrackName, TrackSoundIcon } from './TrackHeaderParts';
import { useTrackReorder } from './useTrackReorder';
import { useArrangementMenus } from './useArrangementMenus';
import { useTimelineLanes } from './useTimelineLanes';

const toolBtn =
  'inline-flex size-5 items-center justify-center rounded text-ink hover:bg-panel-2 disabled:cursor-not-allowed disabled:opacity-40 motion-safe:transition-colors';

const toggle =
  'inline-flex size-6 items-center justify-center rounded border border-line text-xs font-semibold motion-safe:transition-colors';

function MixControls({ track }: { track: Track }) {
  const { actions } = useSession();
  const mix = mixOf(track);
  return (
    <div className="flex items-center gap-1" role="group" aria-label={`Mix for ${track.name}`}>
      <button
        type="button"
        className={`${toggle} ${mix.mute ? 'border-accent bg-accent text-accent-ink' : 'bg-panel text-ink hover:bg-panel-2'}`}
        aria-pressed={mix.mute}
        aria-label={`Mute ${track.name}`}
        title="Mute"
        onClick={() => void actions.setMute(track.id, !mix.mute)}
      >
        M
      </button>
      <button
        type="button"
        className={`${toggle} ${mix.solo ? 'border-accent bg-accent text-accent-ink' : 'bg-panel text-ink hover:bg-panel-2'}`}
        aria-pressed={mix.solo}
        aria-label={`Solo ${track.name}`}
        title="Solo"
        onClick={() => void actions.setSolo(track.id, !mix.solo)}
      >
        S
      </button>
      <MixSlider
        label={`Volume of ${track.name}`}
        className="h-6 min-w-0 flex-1 accent-accent"
        value={mix.volume}
        min={0}
        max={config.volumeMax}
        step={config.volumeStep}
        neutral={config.volumeUnity}
        text={volumeText}
        onCommit={(v) => actions.setVolume(track.id, v)}
      />
      <MixSlider
        label={`Pan of ${track.name}`}
        className="h-6 w-16 shrink-0 accent-accent"
        value={mix.pan}
        min={-config.panMax}
        max={config.panMax}
        step={config.panStep}
        neutral={0}
        text={panText}
        onCommit={(v) => actions.setPan(track.id, v)}
      />
    </div>
  );
}

function InstrumentSelect({ track }: { track: Track }) {
  const { actions, sounds } = useSession();
  const known = track.instrument === null || track.instrument === INSTRUMENT_SINE || track.instrument === INSTRUMENT_SAW;
  const voices = sounds.catalogue?.entries.filter((e) => e.kind === 'builtin') ?? [];
  const rest = sounds.catalogue?.entries.filter((e) => e.kind !== 'builtin') ?? [];
  const picked = track.sound ? `sound:${track.sound}` : (track.instrument ?? '');
  return (
    <select
      aria-label={`Instrument for ${track.name}`}
      className={`${field} w-24 px-1 max-[900px]:w-20`}
      value={picked}
      onChange={(e) => {
        const v = e.target.value;
        if (v.startsWith('sound:')) void actions.setSound(track.id, v.slice('sound:'.length));
        else void actions.setInstrument(track.id, v || null);
      }}
    >
      <option value={INSTRUMENT_SINE}>Sine</option>
      <option value={INSTRUMENT_SAW}>Saw</option>
      {!known && !track.sound && <option value={track.instrument ?? ''}>Custom sound</option>}
      {track.instrument === null && !track.sound && <option value="">No sound</option>}
      {track.sound && !voices.some((e) => e.id === track.sound) && (
        <option value={`sound:${track.sound}`}>Custom sound</option>
      )}
      {voices.length > 0 && (
        <optgroup label="Built in voices">
          {voices.map((e) => (
            <option key={e.id} value={`sound:${e.id}`}>
              {e.name}
            </option>
          ))}
        </optgroup>
      )}
      {/* Sample packs cannot load in the browser yet, so they are shown but cannot be picked. */}
      {rest.length > 0 && (
        <optgroup label="Sample packs (not playable yet)">
          {rest.map((e) => (
            <option key={e.id} disabled>
              {e.name} ({e.kind})
            </option>
          ))}
        </optgroup>
      )}
      {sounds.error && (
        <optgroup label="Sound catalogue">
          <option disabled>{sounds.error}</option>
        </optgroup>
      )}
    </select>
  );
}

export function ArrangementPanel({ onOpenClip }: { onOpenClip(regionId: string): void }) {
  const session = useSession();
  const { store, actions } = session;
  const snap = useSnapshot();
  const compactRows = useSyncExternalStore(subscribePrefs, () => getPrefs().density === 'compact');
  const importRef = useRef<HTMLInputElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const paletteRef = useRef<Palette | null>(null);
  const ctrlRef = useRef<ArrangementController | null>(null);
  const openRef = useRef(onOpenClip);
  useEffect(() => {
    openRef.current = onOpenClip;
  }, [onOpenClip]);
  const lanes = useTimelineLanes(ctrlRef, paletteRef);
  const order = useMemo(() => snap.project.tracks.map((t) => t.id), [snap.project.tracks]);
  const reorder = useTrackReorder(listRef, store, order);
  const x = useArrangementMenus(ctrlRef, (id) => openRef.current(id));
  const menuRef = useRef(x.onClipMenu);
  useEffect(() => {
    menuRef.current = x.onClipMenu;
  }, [x.onClipMenu]);

  const onPalette = useCallback((p: Palette) => {
    paletteRef.current = p;
    ctrlRef.current?.setScrollY(listRef.current?.scrollTop ?? 0);
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctrl = new ArrangementController({
      canvas,
      store,
      audio: session.audio,
      actions,
      transport: session.transport,
      getPalette: () => paletteRef.current,
      openClip: (id) => openRef.current(id),
      onMenu: (target, at) => menuRef.current(target, at),
      scrollListBy: (dy) => {
        if (listRef.current) listRef.current.scrollTop += dy;
      },
    });
    ctrlRef.current = ctrl;
    return () => {
      ctrl.destroy();
      ctrlRef.current = null;
    };
  }, [store, actions, session.audio, session.transport]);

  return (
    <section aria-label="Arrangement" className="flex min-h-0 flex-1 bg-panel">
      <PaletteProbe onPalette={onPalette} />
      <div className="flex w-80 shrink-0 flex-col border-r border-line max-[900px]:w-52">
        {lanes.labels}
        <div className="flex h-7 items-center justify-between px-2">
          <h2 className="text-[13px] font-semibold text-ink-dim">Tracks</h2>
          <button type="button" className={btnSmall} onClick={() => void actions.addTrack()}>
            <PlusIcon size={14} />
            Add track
          </button>
        </div>
        <div
          ref={listRef}
          className="min-h-0 flex-1 overflow-y-auto"
          onScroll={(e) => ctrlRef.current?.setScrollY(e.currentTarget.scrollTop)}
          {...x.listProps}
        >
          <ul className="m-0 list-none p-0">
            {snap.project.tracks.map((t, i) => (
              <li
                key={t.id}
                data-reorder={reorder.indicator(i)}
                data-moving={reorder.dragging === t.id}
                aria-current={t.id === snap.selectedTrack}
                {...x.trackAttr(t.id)}
                data-drop={x.hint === t.id}
                className={`data-[drop=true]:ring-2 data-[drop=true]:ring-inset data-[drop=true]:ring-accent flex flex-col justify-center gap-1 border-b border-line px-2 [content-visibility:auto] ${
                  compactRows ? 'h-[60px] [contain-intrinsic-size:auto_60px]' : 'h-16 [contain-intrinsic-size:auto_64px]'
                } ${t.id === snap.selectedTrack ? 'bg-roll-row' : ''} data-[moving=true]:opacity-60 data-[reorder=up]:shadow-[inset_0_2px_0_0_var(--color-ink)] data-[reorder=down]:shadow-[inset_0_-2px_0_0_var(--color-ink)]`}
                onPointerDown={() => store.selectTrack(t.id)}
              >
                <div className="flex items-center gap-1">
                  <Grip track={t} index={i} count={snap.project.tracks.length} pointerProps={reorder.pointerProps(t.id)} onKey={reorder.keyProps(t.id)} />
                  <ColorChip track={t} entries={session.sounds.catalogue?.entries ?? []} />
                  <TrackSoundIcon track={t} entries={session.sounds.catalogue?.entries ?? []} />
                  <TrackName track={t} onCommit={(v) => void actions.renameTrack(t.id, v)} />
                  <InstrumentSelect track={t} />
                  <MenuButton track={t} onOpen={x.openTrackMenu} />
                  <button
                    type="button"
                    className={compact(iconBtn)}
                    aria-label={`Delete ${t.name}`}
                    title="Delete track"
                    onClick={() => void actions.deleteTrack(t.id)}
                  >
                    <TrashIcon size={16} />
                  </button>
                </div>
                <MixControls track={t} />
              </li>
            ))}
          </ul>
          {x.dragging && (
            <p
              data-testid="drop-new-track"
              data-drop={x.hint === 'new-track'}
              className="m-2 rounded-md border border-dashed border-line p-3 text-center text-xs text-ink-dim data-[drop=true]:border-accent data-[drop=true]:text-ink"
            >
              Drop here to add a track with this sound
            </p>
          )}
          {snap.project.tracks.length === 0 && (
            <>
              <EmptyState
                title="No tracks yet"
                hint="Add a track to get a 4 bar clip, or open a project file."
                primary={{ label: 'Add a track', icon: <PlusIcon size={16} />, onClick: () => void actions.addTrack() }}
                secondary={{ label: 'Open a project file', icon: <UploadIcon size={16} />, onClick: () => importRef.current?.click() }}
              />
              <input
                ref={importRef}
                type="file"
                accept="application/json,.json"
                className="hidden"
                aria-label="Open a project file"
                tabIndex={-1}
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) void actions.importJson(file);
                  e.target.value = '';
                }}
              />
            </>
          )}
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        {lanes.strip}
        <div className="relative min-h-0 flex-1" {...x.laneProps}>
        <canvas
          ref={canvasRef}
          tabIndex={0}
          role="application"
          id="arrangement-canvas"
          data-own-keys=""
          aria-label="Arrangement. Arrow keys move between clips and tracks, Shift plus arrows add clips to the selection, Enter opens the clip in Notes, N adds a clip, S splits at the playhead, Delete removes, F fits the selection, Shift F fits the song, Home goes to the start. Alt plus left or right moves the clip, Alt plus Shift resizes it, Alt plus up or down changes its track."
          className="block h-full w-full touch-none cursor-pointer"
        />
        <div
          role="toolbar"
          aria-label="Arrangement view"
          className="absolute right-1 top-0.5 flex h-6 items-center gap-0.5 rounded-md border border-line bg-panel px-0.5"
        >
          {lanes.toolbar}
          <button type="button" className={toolBtn} aria-label="Zoom out" title="Zoom out (minus)" onClick={() => ctrlRef.current?.zoomBy(1 / config.zoomStep)}>
            <MinusIcon size={14} />
          </button>
          <button type="button" className={toolBtn} aria-label="Zoom in" title="Zoom in (plus)" onClick={() => ctrlRef.current?.zoomBy(config.zoomStep)}>
            <PlusIcon size={14} />
          </button>
          <button type="button" className={`${toolBtn} w-auto px-1.5 text-xs font-medium`} title="Fit the whole song (Shift F)" onClick={() => ctrlRef.current?.fit(false)}>
            Fit
          </button>
          <button
            type="button"
            className={toolBtn}
            aria-label="Loop the selected clips"
            title="Loop the selected clips (or the whole song)"
            onClick={() => void session.transport.loopSelection(true)}
          >
            <LoopIcon size={14} />
          </button>
          <button
            type="button"
            className={toolBtn}
            aria-label="Split at the playhead"
            title="Split the selected clips at the playhead (S)"
            disabled={snap.selectedRegions.size === 0}
            onClick={() => void ctrlRef.current?.splitAtPlayhead()}
          >
            <ScissorsIcon size={14} />
          </button>
          <button
            type="button"
            className={toolBtn}
            aria-label="Duplicate the selected clips"
            title="Duplicate the selected clips after themselves (Ctrl D)"
            disabled={snap.selectedRegions.size === 0}
            onClick={() => ctrlRef.current?.duplicateSelection()}
          >
            <CopyIcon size={14} />
          </button>
          <button
            type="button"
            className={toolBtn}
            aria-label="Repeat the selected clips"
            title="Repeat the selected clips a number of times (Ctrl Shift D)"
            disabled={snap.selectedRegions.size === 0}
            onClick={() => ctrlRef.current?.repeatSelection()}
          >
            <RepeatNIcon size={14} />
          </button>
        </div>
        </div>
      </div>
      {x.menu}
      {lanes.overlays}
    </section>
  );
}
