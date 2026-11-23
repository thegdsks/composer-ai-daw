// Events waiting for the next block. Same time events keep arrival order (a note off sent before a
// note on at one tick must reach the plugin first), so the sort key is (time, arrival number).

export interface ParamEvent {
  /** Sample offset inside the block. */
  time: number;
  seq: number;
  id: number;
  value: number;
}

export class EventQueue {
  private items: ParamEvent[] = [];
  private seq = 0;
  /** Events refused because the queue was full; shown in the stats, never silent. */
  dropped = 0;

  constructor(private readonly max: number) {}

  push(time: number, id: number, value: number): boolean {
    if (this.items.length >= this.max) {
      this.dropped++;
      return false;
    }
    this.items.push({ time, seq: this.seq++, id, value });
    return true;
  }

  get length(): number {
    return this.items.length;
  }

  /** Takes every event with time below `limit` (a block length), in delivery order. */
  take(limit: number): ParamEvent[] {
    if (this.items.length === 0) return [];
    const now: ParamEvent[] = [];
    const later: ParamEvent[] = [];
    for (const e of this.items) {
      if (e.time < limit) now.push(e);
      else later.push({ ...e, time: e.time - limit });
    }
    this.items = later;
    return now.sort((a, b) => a.time - b.time || a.seq - b.seq);
  }
}
