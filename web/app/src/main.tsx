import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import { applyPrefs } from './prefs';
import { applyLibrary } from './ui/libraryState';
import { boot } from './session';
import { App } from './ui/App';

applyPrefs();
applyLibrary();
const container = document.getElementById('root');
if (!container) throw new Error('missing #root');
const root = createRoot(container);

function Failure({ message }: { message: string }) {
  return (
    <main className="mx-auto max-w-xl p-8">
      <h1 className="text-xl font-semibold">Composer AI DAW could not start</h1>
      <p className="mt-2 text-ink-dim" role="alert">
        {message}
      </p>
    </main>
  );
}

root.render(<p className="p-8 text-ink-dim">Loading the sound engine…</p>);
boot(window.location.search, window.location.hash)
  .then((session) => {
    // The token only needs to be in the address bar long enough to be read once.
    if (window.location.hash.includes('token=')) history.replaceState(null, '', window.location.pathname + window.location.search);
    root.render(
      <StrictMode>
        <App session={session} />
      </StrictMode>,
    );
  })
  .catch((e: unknown) => root.render(<Failure message={e instanceof Error ? e.message : 'Unknown error'} />));
