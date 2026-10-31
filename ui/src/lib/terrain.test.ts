import { describe, expect, it } from 'vitest';
import { fromKm, linePaths, mercator, shade, terrarium, tilesFor, toKm } from './terrain';

const boulder = { lat: 40.015, lon: -105.27 };
const palette = { plain: '#202833', peak: '#404850', shadow: '#0c1014', light: '#9aa5b1' };

describe('terrain', () => {
  it('decodes terrarium heights', () => {
    expect(terrarium(128, 0, 0)).toBe(0);
    // Boulder is about 1650 m up
    expect(terrarium(134, 114, 0)).toBe(1650);
  });

  it('round-trips km and degrees', () => {
    const [lon, lat] = fromKm(boulder, 30, -20);
    const [x, y] = toKm(boulder, lon, lat);
    expect(x).toBeCloseTo(30, 6);
    expect(y).toBeCloseTo(-20, 6);
  });

  it('puts Boulder in tile 8/53/96', () => {
    const [x, y] = mercator(boulder.lon, boulder.lat, 8).map((v) => Math.floor(v / 256));
    expect([x, y]).toEqual([53, 96]);
  });

  it('covers the 400 x 150 km frame with a handful of zoom 8 tiles', () => {
    const tiles = tilesFor({ site: boulder, halfWKm: 200, halfHKm: 75 }, 8);
    expect(tiles).toContainEqual({ x: 53, y: 96 });
    expect(tiles.length).toBeGreaterThanOrEqual(4);
    expect(tiles.length).toBeLessThanOrEqual(12);
  });

  it('leaves flat ground the plain colour and missing ground transparent', () => {
    const h = new Float32Array([1000, 1000, 1000, 1000, 1000, 1000, NaN, 1000, 1000]);
    const px = shade(h, 3, 3, 0.5, palette);
    expect([...px.slice(16, 20)]).toEqual([0x20, 0x28, 0x33, 255]);
    expect(px[6 * 4 + 3]).toBe(0);
  });

  it('lights slopes facing north-west and darkens the others', () => {
    // rises to the south-east, so the slope faces north-west, into the light
    const h = new Float32Array(9).map((_, i) => 1000 + ((i % 3) + Math.floor(i / 3)) * 40);
    const facing = shade(h, 3, 3, 0.5, palette)[16];
    const away = shade(h.map((v) => 2000 - v), 3, 3, 0.5, palette)[16];
    expect(facing).toBeGreaterThan(0x20);
    expect(away).toBeLessThan(0x20);
  });

  it('keeps only lines that cross the frame', () => {
    const lines = {
      states: [[[-105.5, 40], [-105, 40]]] as [number, number][][],
      roads: [
        { major: 1, c: [[-105.3, 39.9], [-105.2, 40.1]] as [number, number][] },
        { major: 0, c: [[-80, 30], [-79, 31]] as [number, number][] },
      ],
    };
    const p = linePaths(lines, { site: boulder, halfWKm: 200, halfHKm: 75 });
    expect(p.states.startsWith('M')).toBe(true);
    expect(p.major.startsWith('M')).toBe(true);
    expect(p.minor).toBe('');
  });
});
