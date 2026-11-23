export interface GainPluginOptions {
  /** Leave out one required export. */
  omit?: 'entry' | 'malloc';
  /** Import a function from a namespace plugins may not use. */
  badImport?: boolean;
  /** Declared memory pages (default 4). */
  memoryPages?: number;
}

export function buildGainPlugin(opts?: GainPluginOptions): Uint8Array;
