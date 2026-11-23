// Turns stable daemon error codes into sentences a beginner can act on. Unknown codes keep
// the code and the daemon's own text so nothing is hidden.
import type { BackendError } from '../backend/types';

const PLAIN: Record<string, string> = {
  file_not_found: 'There is no file with that name in the daemon folder.',
  file_exists: 'A file with that name already exists. Tick Replace existing file to overwrite it.',
  file_too_large: 'That file is too big for the daemon to read.',
  path_invalid: 'That file name cannot be used. Use a plain name without slashes.',
  path_outside_base: 'That name points outside the daemon folder.',
  files_dir_unavailable: 'The daemon folder is not available. Check the daemon is running there.',
  catalogue_unavailable: 'The sound catalogue file is missing or unreadable on the daemon.',
  bad_magic: 'That file is not a Composer AI DAW project.',
  truncated: 'That file is cut short and cannot be opened.',
  unsupported_version: 'That project was saved by a newer version of Composer AI DAW.',
  bad_header: 'That project file is damaged (header).',
  manifest_corrupt: 'That project file is damaged (manifest).',
  section_corrupt: 'That project file is damaged (a section fails its check).',
  missing_section: 'That project file is incomplete.',
  limit_exceeded: 'That project is larger than the daemon allows.',
  malformed: 'That project file could not be read.',
  duplicate_id: 'That project file repeats an id and was refused.',
  dangling_reference: 'That project file points at something that is missing.',
  out_of_range: 'That project file holds a value out of range.',
  inconsistent: 'That project file contradicts itself and was refused.',
  migration_failed: 'That older project could not be upgraded.',
  connection_closed: 'The daemon connection closed. Reload the page to reconnect.',
  timeout: 'The daemon did not answer in time.',
};

export function plainError(e: BackendError): string {
  return PLAIN[e.code] ?? `${e.code}: ${e.message}`;
}
