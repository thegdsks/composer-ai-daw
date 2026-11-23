import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { config } from '../config';
import { focusReturn } from './focusReturn';
import { isMenuKey, menuTarget, placeMenu, type MenuItem } from './menuModel';

export interface MenuRequest {
  label: string;
  items: readonly MenuItem[];
  x: number;
  y: number;
}

const pageFocus = {
  get activeElement(): HTMLElement | null {
    return document.activeElement instanceof HTMLElement ? document.activeElement : null;
  },
};

/**
 * One context menu for the whole app: a popover in the top layer (outside click and Escape close it),
 * role=menu with arrow, Home, End and letter keys, focus on the first item, focus back to the opener.
 */
export function ContextMenu(props: { request: MenuRequest | null; onClose(): void }) {
  const { request, onClose } = props;
  const panel = useRef<HTMLDivElement>(null);
  const live = useRef(false);
  const [focus] = useState(() => focusReturn(pageFocus));

  useEffect(() => {
    const el = panel.current;
    if (!el || !request) return;
    focus.capture();
    live.current = true;
    el.style.setProperty('--menu-x', `${request.x}px`);
    el.style.setProperty('--menu-y', `${request.y}px`);
    el.showPopover();
    const box = el.getBoundingClientRect();
    const at = placeMenu(
      { x: request.x, y: request.y },
      { w: box.width, h: box.height },
      { w: window.innerWidth, h: window.innerHeight },
      config.menuViewportMarginPx,
    );
    el.style.setProperty('--menu-x', `${at.x}px`);
    el.style.setProperty('--menu-y', `${at.y}px`);
    el.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    return () => {
      if (el.matches(':popover-open')) el.hidePopover();
    };
  }, [request, focus]);

  const buttons = (): HTMLElement[] => Array.from(panel.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);

  /** Closes now, hands focus back, then lets the caller carry on (a dialog may take focus next). */
  const finish = useCallback((): void => {
    if (!live.current) return;
    live.current = false;
    const el = panel.current;
    if (el?.matches(':popover-open')) el.hidePopover();
    focus.restore();
    onClose();
  }, [focus, onClose]);

  const choose = (item: MenuItem): void => {
    if (item.disabled) return;
    finish();
    item.run();
  };

  const onKeyDown = (e: KeyboardEvent): void => {
    if (e.key === 'Tab') {
      e.preventDefault();
      finish();
      return;
    }
    const all = buttons();
    const at = all.indexOf(document.activeElement as HTMLElement);
    const next = menuTarget(
      all.map((b) => b.dataset.label ?? ''),
      at,
      e.key,
    );
    if (next === null || e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    all[next]?.focus();
  };

  return (
    <div
      ref={panel}
      popover="auto"
      role="menu"
      aria-label={request?.label ?? 'Menu'}
      data-testid="context-menu"
      onKeyDown={onKeyDown}
      onToggle={(e) => {
        if (e.nativeEvent instanceof ToggleEvent && e.nativeEvent.newState === 'closed') finish();
      }}
      className="fixed inset-auto left-(--menu-x) top-(--menu-y) m-0 min-w-52 max-w-80 flex-col gap-0.5 rounded-md border border-line bg-panel p-1 text-ink shadow-md [&:popover-open]:flex"
    >
      {request?.items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="menuitem"
          tabIndex={-1}
          aria-disabled={item.disabled || undefined}
          data-label={item.label}
          data-menu-id={item.id}
          className={`flex min-h-9 items-center gap-3 rounded px-3 py-1 text-left text-sm hover:bg-panel-2 focus-visible:bg-panel-2 aria-disabled:cursor-not-allowed aria-disabled:text-ink-dim ${
            item.separated ? 'mt-1 border-t border-line pt-1.5' : ''
          } ${item.danger ? 'text-danger' : ''}`}
          onClick={() => choose(item)}
        >
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate">{item.label}</span>
            {item.hint && <span className="text-xs text-ink-dim">{item.hint}</span>}
            {item.disabled && item.reason && <span className="text-xs text-ink-dim">{item.reason}</span>}
          </span>
          {item.shortcut && <kbd className="shrink-0 font-sans text-xs text-ink-dim">{item.shortcut}</kbd>}
        </button>
      ))}
    </div>
  );
}

/**
 * Opens a menu from a right click, Shift+F10, the Menu key, or a long press on touch.
 * `build` returns the menu for the event target, or null for none.
 */
export function useMenuTrigger(build: (origin: HTMLElement) => MenuRequestInput | null) {
  const [request, setRequest] = useState<MenuRequest | null>(null);
  const press = useRef<{ timer: ReturnType<typeof setTimeout>; x: number; y: number } | null>(null);

  const open = useCallback(
    (origin: HTMLElement, at: { x: number; y: number } | null): boolean => {
      const made = build(origin);
      if (!made) return false;
      const box = origin.getBoundingClientRect();
      setRequest({ label: made.label, items: made.items, x: at?.x ?? box.left + 8, y: at?.y ?? box.bottom });
      return true;
    },
    [build],
  );

  const cancelPress = (): void => {
    if (press.current) clearTimeout(press.current.timer);
    press.current = null;
  };

  useEffect(() => cancelPress, []);

  const props = {
    onContextMenu(e: { clientX: number; clientY: number; target: EventTarget; preventDefault(): void; currentTarget: HTMLElement }): void {
      const origin = e.target instanceof HTMLElement ? e.target : e.currentTarget;
      // Text fields keep the browser menu (cut, copy, paste); Shift+F10 still reaches ours.
      if (origin.closest('input, textarea, select')) return;
      // A menu opened by the Menu key can arrive with no pointer position (0, 0): anchor on the element.
      const keyboard = e.clientX === 0 && e.clientY === 0;
      if (open(origin, keyboard ? null : { x: e.clientX, y: e.clientY })) e.preventDefault();
    },
    onKeyDown(e: KeyboardEvent<HTMLElement>): void {
      if (!isMenuKey(e)) return;
      const origin = e.target instanceof HTMLElement ? e.target : e.currentTarget;
      if (open(origin, null)) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    onPointerDown(e: PointerEvent<HTMLElement>): void {
      if (e.pointerType !== 'touch') return;
      cancelPress();
      const { clientX: x, clientY: y } = e;
      const origin = e.target instanceof HTMLElement ? e.target : e.currentTarget;
      press.current = { x, y, timer: setTimeout(() => open(origin, { x, y }), config.menuLongPressMs) };
    },
    onPointerMove(e: PointerEvent<HTMLElement>): void {
      const p = press.current;
      if (p && Math.hypot(e.clientX - p.x, e.clientY - p.y) > config.menuLongPressSlopPx) cancelPress();
    },
    onPointerUp: cancelPress,
    onPointerCancel: cancelPress,
  };

  return { request, close: useCallback(() => setRequest(null), []), props, openAt: setRequest };
}

export interface MenuRequestInput {
  label: string;
  items: readonly MenuItem[];
}
