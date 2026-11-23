// Client side id generator: 128 random bits as 32 lower case hex digits, the shape the
// command API requires. The all zero and all one values are avoided (reserved by the model).

const HEX_DIGITS = 32;

export function newId(random: (bytes: Uint8Array<ArrayBuffer>) => void = fill): string {
  const bytes = new Uint8Array(HEX_DIGITS / 2);
  for (;;) {
    random(bytes);
    let out = '';
    for (const b of bytes) out += b.toString(16).padStart(2, '0');
    if (!/^0+$/.test(out) && !/^f+$/.test(out)) return out;
  }
}

function fill(bytes: Uint8Array<ArrayBuffer>): void {
  crypto.getRandomValues(bytes);
}

export function isId(value: string): boolean {
  return /^[0-9a-f]{32}$/.test(value);
}
