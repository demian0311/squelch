// What draws the scrolling waterfall: WebGL when the machine has it, a 2D canvas otherwise.
import { CpuWaterfall } from './cpu';
import { GlWaterfall } from './gl';

export interface Renderer {
  /** Size in device pixels; each row is `rowPx` tall. */
  resize(w: number, h: number, rowPx: number): void;
  /** 256 colours, three bytes each, from the theme. */
  setPalette(lut: Uint8ClampedArray): void;
  /** One spectrum row: a byte per FFT bin over a capture `rate` Hz wide around `centerHz`. The view is where it lands on a 2D canvas. */
  push(bins: Uint8Array, centerHz: number, rate: number, vs: number, ve: number): void;
  /** Show the history for this view. */
  draw(vs: number, ve: number): void;
}

export function makeRenderer(canvas: HTMLCanvasElement): Renderer {
  try {
    return new GlWaterfall(canvas);
  } catch (e) {
    console.error('waterfall: no WebGL, drawing on a 2D canvas', e);
    // a canvas that has handed out a WebGL context cannot give a 2D one, so the fallback gets a fresh element
    const fresh = canvas.cloneNode() as HTMLCanvasElement;
    canvas.replaceWith(fresh);
    return new CpuWaterfall(fresh);
  }
}
