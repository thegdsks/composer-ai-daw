// Text for the mixer controls. Pure, so the wording is testable.
import { config } from '../config';

/** Unity gain reads 100 percent. */
export function volumeText(permille: number): string {
  return `${Math.round((permille / config.volumeUnity) * 100)} percent`;
}

export function panText(permille: number): string {
  if (permille === 0) return 'Center';
  const pct = Math.round((Math.abs(permille) / config.panMax) * 100);
  return `${permille < 0 ? 'Left' : 'Right'} ${pct} percent`;
}
