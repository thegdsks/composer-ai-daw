import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

// Chrome files may not name a hue. Data and signal colors live only in the allowlisted file.
const ALLOW = new Set(['src/data-colors.css']);
const HUES = 'red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose';
const HUE_CLASS = new RegExp(`\\b(?:text|bg|border|ring|outline|fill|stroke|from|to|via|divide|accent|caret|shadow)-(?:${HUES})-\\d`);

function walk(dir: string, out: string[]): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(tsx?|css)$/.test(name)) out.push(p);
  }
  return out;
}

/** The first color literal with visible chroma, or null. Equal channel grays pass. */
function hueLiteral(text: string): string | null {
  for (const m of text.matchAll(/#([0-9a-fA-F]{3,8})\b/g)) {
    let h = m[1] ?? '';
    if (h.length <= 4) h = [...h].map((c) => c + c).join('');
    const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
    if (r !== g || g !== b) return m[0];
  }
  for (const m of text.matchAll(/rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/g)) {
    if (m[1] !== m[2] || m[2] !== m[3]) return m[0];
  }
  for (const m of text.matchAll(/(?:hsla?|oklch|oklab|lch)\(\s*[\d.]+%?\s+([\d.]+)/g)) {
    if (parseFloat(m[1] ?? '0') > 0.001) return m[0];
  }
  const cls = HUE_CLASS.exec(text);
  return cls ? cls[0] : null;
}

describe('monochrome chrome', () => {
  it('has no hue-bearing color literal outside the data color allowlist', () => {
    const files = [...walk('src/ui', []), 'src/index.css'];
    const bad = files
      .filter((f) => !ALLOW.has(f))
      .map((f) => [f, hueLiteral(readFileSync(f, 'utf8'))] as const)
      .filter(([, hit]) => hit !== null);
    expect(bad).toEqual([]);
  });
});
