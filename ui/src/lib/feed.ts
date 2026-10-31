// Decodes the backend's `feed` answer (src-tauri/src/feed.rs, `Feed::take`), little endian:
// u32 status length (0: unchanged), the status JSON, then per row a u32 length and the row,
// itself an f64 centre and one byte per bin.

export interface FeedRow {
  centerHz: number;
  bins: Uint8Array;
}

const text = new TextDecoder();

export function decodeFeed<S>(buf: ArrayBuffer): { status: S | null; rows: FeedRow[] } {
  const view = new DataView(buf);
  const n = view.getUint32(0, true);
  const status = n ? (JSON.parse(text.decode(new Uint8Array(buf, 4, n))) as S) : null;
  const rows: FeedRow[] = [];
  for (let at = 4 + n; at + 4 <= buf.byteLength; ) {
    const len = view.getUint32(at, true);
    rows.push({ centerHz: view.getFloat64(at + 4, true), bins: new Uint8Array(buf, at + 12, len - 8) });
    at += 4 + len;
  }
  return { status, rows };
}
