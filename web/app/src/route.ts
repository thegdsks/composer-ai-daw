// Hash routes: #/start is the goal sheet, #/p/<project id> is the workspace.
export type Route = { kind: 'start' } | { kind: 'project'; id: string } | { kind: 'none' };

export function parseRoute(hash: string): Route {
  const path = hash.replace(/^#/, '');
  if (path === '/start') return { kind: 'start' };
  const m = /^\/p\/([0-9a-f]+)$/.exec(path);
  if (m?.[1]) return { kind: 'project', id: m[1] };
  return { kind: 'none' };
}

const listeners = new Set<() => void>();

export function subscribeRoute(l: () => void): () => void {
  listeners.add(l);
  window.addEventListener('hashchange', l);
  window.addEventListener('popstate', l);
  return () => {
    listeners.delete(l);
    window.removeEventListener('hashchange', l);
    window.removeEventListener('popstate', l);
  };
}

export const currentHash = (): string => window.location.hash;

/** Moves to a route. `replace` keeps Back from returning to a sheet the user just closed. */
export function goTo(route: Route, replace: boolean): void {
  const hash = route.kind === 'start' ? '#/start' : route.kind === 'project' ? `#/p/${route.id}` : '';
  const url = window.location.pathname + window.location.search + hash;
  if (replace) history.replaceState(null, '', url);
  else history.pushState(null, '', url);
  for (const l of listeners) l();
}
