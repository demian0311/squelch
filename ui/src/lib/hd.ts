// HD Radio: which program a station's chip selects, and laying out the traffic map.
import type { HdImage } from './api';

/** Next state of a station's HD chip: off → HD1 → HD2 … → off. */
export function nextHd(current: number | null, programs: number[]): number | null {
  const list = programs.length ? programs : [0];
  if (current === null) return list[0];
  const later = list.filter((p) => p > current);
  return later.length ? later[0] : null;
}

export const hdLabel = (program: number) => `HD${program + 1}`;

/** Traffic tiles placed on their 3×3 grid, row-major; a missing tile is null. */
export function trafficGrid(images: HdImage[]): (string | null)[] | null {
  const tiles = images.filter((i) => i.kind === 'traffic' && i.row >= 1 && i.row <= 3 && i.col >= 1 && i.col <= 3);
  if (!tiles.length) return null;
  const grid: (string | null)[] = Array(9).fill(null);
  for (const t of tiles) grid[(t.row - 1) * 3 + t.col - 1] = t.data_url;
  return grid;
}

/** What the panel says about the decoder. */
export function hdState(hd: { error: string | null; synced: boolean; digital: boolean }): string {
  if (hd.error) return hd.error;
  if (!hd.synced) return 'Looking for the digital signal…';
  if (!hd.digital) return 'Locked on — digital audio starts in a moment';
  return 'Playing digital audio';
}
