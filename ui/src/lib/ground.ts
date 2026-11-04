// Shaded terrain as a PNG, from elevation tiles: best effort, cached in memory and on disk. No tiles, no ground.
import * as api from './api';
import { fromKm, groundKey, mercator, shade, terrarium, tilesFor, TILE, type Frame, type Palette } from './terrain';

// elevation tiles at zoom 8 are about 470 m a pixel at 40° N, so each wider ground takes tiles one zoom out and half the pixels a km
export const GROUNDS = [
  { halfHKm: 75, zoom: 8, pxPerKm: 2 },
  { halfHKm: 150, zoom: 7, pxPerKm: 1 },
  { halfHKm: 300, zoom: 6, pxPerKm: 0.5 },
];

export const groundCache = new Map<string, string>();

const loadImage = (src: string) =>
  new Promise<HTMLImageElement | null>((res) => {
    const i = new Image();
    i.onload = () => res(i);
    i.onerror = () => res(null);
    i.src = src;
  });

/** The frame's shaded terrain as a PNG data URL, or null with no tiles at all. */
export async function drawGround(f: Frame, palette: Palette, { zoom: ZOOM, pxPerKm: PX_PER_KM }: (typeof GROUNDS)[number]): Promise<string | null> {
  const tiles = tilesFor(f, ZOOM);
  const urls = await Promise.all(tiles.map((t) => api.terrainTile(ZOOM, t.x, t.y).catch(() => null)));
  if (urls.every((u) => !u)) return null;
  const x0 = Math.min(...tiles.map((t) => t.x));
  const y0 = Math.min(...tiles.map((t) => t.y));
  const mw = (Math.max(...tiles.map((t) => t.x)) - x0 + 1) * TILE;
  const mh = (Math.max(...tiles.map((t) => t.y)) - y0 + 1) * TILE;
  const mosaic = document.createElement('canvas');
  mosaic.width = mw;
  mosaic.height = mh;
  const mc = mosaic.getContext('2d', { willReadFrequently: true })!;
  const images = await Promise.all(urls.map((u) => (u ? loadImage(u) : null)));
  images.forEach((img, i) => img && mc.drawImage(img, (tiles[i].x - x0) * TILE, (tiles[i].y - y0) * TILE));
  const px = mc.getImageData(0, 0, mw, mh).data;

  const w = Math.round(2 * f.halfWKm * PX_PER_KM);
  const h = Math.round(2 * f.halfHKm * PX_PER_KM);
  const heights = new Float32Array(w * h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const [lon, lat] = fromKm(f.site, (x + 0.5) / PX_PER_KM - f.halfWKm, (y + 0.5) / PX_PER_KM - f.halfHKm);
      const [mx, my] = mercator(lon, lat, ZOOM);
      const ix = Math.floor(mx - x0 * TILE);
      const iy = Math.floor(my - y0 * TILE);
      const k = (iy * mw + ix) * 4;
      heights[y * w + x] = ix < 0 || iy < 0 || ix >= mw || iy >= mh || px[k + 3] === 0 ? NaN : terrarium(px[k], px[k + 1], px[k + 2]);
    }
  }
  const out = document.createElement('canvas');
  out.width = w;
  out.height = h;
  out.getContext('2d')!.putImageData(new ImageData(shade(heights, w, h, 1 / PX_PER_KM, palette), w, h), 0, 0);
  return out.toDataURL('image/png');
}

/** The frame's ground from memory, the disk cache, or drawn and saved. */
export async function loadGround(f: Frame, p: Palette, level: (typeof GROUNDS)[number]): Promise<string | null> {
  const key = groundKey(f, p);
  const cached = groundCache.get(key);
  if (cached) return cached;
  let url = await api.ground(key).catch(() => null);
  if (!url) {
    url = await drawGround(f, p, level);
    if (url) api.saveGround(key, url).catch(() => {});
  }
  if (url) groundCache.set(key, url);
  return url;
}
