import { describe, expect, it } from 'vitest';
import { decodeFeed } from './feed';

function encode(status: string, rows: { centerHz: number; bins: number[] }[]): ArrayBuffer {
  const json = new TextEncoder().encode(status);
  const size = 4 + json.length + rows.reduce((a, r) => a + 4 + 8 + r.bins.length, 0);
  const buf = new ArrayBuffer(size);
  const view = new DataView(buf);
  view.setUint32(0, json.length, true);
  new Uint8Array(buf, 4).set(json);
  let at = 4 + json.length;
  for (const r of rows) {
    view.setUint32(at, 8 + r.bins.length, true);
    view.setFloat64(at + 4, r.centerHz, true);
    new Uint8Array(buf, at + 12).set(r.bins);
    at += 12 + r.bins.length;
  }
  return buf;
}

describe('decodeFeed', () => {
  it('reads the status and every row, at odd offsets too', () => {
    const { status, rows } = decodeFeed<{ a: number }>(
      encode('{"a":1}', [
        { centerHz: 146_000_000, bins: [1, 2, 3] },
        { centerHz: 146_500_000, bins: [4, 5] },
      ]),
    );
    expect(status).toEqual({ a: 1 });
    expect(rows.map((r) => [r.centerHz, [...r.bins]])).toEqual([
      [146_000_000, [1, 2, 3]],
      [146_500_000, [4, 5]],
    ]);
  });

  it('leaves the status alone when it has not changed', () => {
    const { status, rows } = decodeFeed(encode('', [{ centerHz: 1, bins: [9] }]));
    expect(status).toBeNull();
    expect(rows).toHaveLength(1);
  });

  it('answers an empty wait with nothing', () => {
    expect(decodeFeed(encode('', []))).toEqual({ status: null, rows: [] });
  });
});
