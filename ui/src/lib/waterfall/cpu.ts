// 2D-canvas waterfall, for a machine without WebGL. Rows are drawn in view coordinates, so a change of view stretches
// and shifts the history to keep it lined up; each new row scrolls the whole picture down by one row.
import type { Renderer } from './renderer';
import { binAt } from '../radio';

export class CpuWaterfall implements Renderer {
  private ctx: CanvasRenderingContext2D;
  private rowCanvas = document.createElement('canvas');
  private rowCtx: CanvasRenderingContext2D;
  private img: ImageData | null = null;
  private lut: Uint8ClampedArray = new Uint8ClampedArray(256 * 3);
  private rowPx = 1;
  private lastVs = 0;
  private lastVe = 0;
  // which bins each device pixel covers; recomputed only when the view, the capture or the width changes
  private b0s = new Int32Array(0);
  private b1s = new Int32Array(0);
  private rangeKey = '';

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext('2d')!;
    this.ctx.imageSmoothingEnabled = false;
    this.rowCtx = this.rowCanvas.getContext('2d')!;
  }

  resize(w: number, h: number, rowPx: number) {
    this.rowPx = rowPx;
    const { canvas, ctx } = this;
    if (canvas.width === w && canvas.height === h) return;
    const old = canvas.width > 0 && canvas.height > 0 ? ctx.getImageData(0, 0, canvas.width, canvas.height) : null;
    canvas.width = w;
    canvas.height = h;
    ctx.imageSmoothingEnabled = false;
    this.fill(0, 0, w, h);
    if (old) ctx.putImageData(old, 0, 0);
    // each new row is written to this one-line canvas and drawn from there: putImageData straight onto the big canvas
    // costs milliseconds a call in WebKit, drawImage does not
    this.rowCanvas.width = w;
    this.rowCanvas.height = 1;
    this.img = this.rowCtx.createImageData(w, 1);
    this.rangeKey = '';
  }

  private fill(x: number, y: number, w: number, h: number) {
    this.ctx.fillStyle = `rgb(${this.lut[0]} ${this.lut[1]} ${this.lut[2]})`;
    this.ctx.fillRect(x, y, w, h);
  }

  setPalette(lut: Uint8ClampedArray) {
    this.lut = lut;
  }

  private binRanges(W: number, lo: number, n: number, rate: number, vs: number, ve: number) {
    const key = `${W} ${lo} ${n} ${rate} ${vs} ${ve}`;
    if (key === this.rangeKey) return;
    this.rangeKey = key;
    if (this.b0s.length !== W) {
      this.b0s = new Int32Array(W);
      this.b1s = new Int32Array(W);
    }
    const binHz = rate / n;
    const span = ve - vs;
    for (let x = 0; x < W; x++) {
      const b0 = binAt(vs + (x / W) * span, lo, binHz);
      const b1 = binAt(vs + ((x + 1) / W) * span, lo, binHz);
      // b1 < 0 marks a pixel outside the capture
      if (b1 < 0 || b0 >= n) {
        this.b1s[x] = -1;
      } else {
        this.b0s[x] = Math.max(0, b0);
        this.b1s[x] = Math.min(n - 1, Math.max(b0, b1));
      }
    }
  }

  push(bins: Uint8Array, centerHz: number, rate: number, vs: number, ve: number) {
    const { ctx, img, canvas } = this;
    if (!img) return;
    this.draw(vs, ve);
    const W = canvas.width;
    const H = canvas.height;
    this.binRanges(W, centerHz - rate / 2, bins.length, rate, vs, ve);
    const data = img.data;
    const L = this.lut;
    for (let x = 0; x < W; x++) {
      const b1 = this.b1s[x];
      let v = 0;
      if (b1 >= 0) for (let b = this.b0s[x]; b <= b1; b++) if (bins[b] > v) v = bins[b];
      const k = v * 3;
      data[x * 4] = L[k];
      data[x * 4 + 1] = L[k + 1];
      data[x * 4 + 2] = L[k + 2];
      data[x * 4 + 3] = 255;
    }
    const step = this.rowPx;
    ctx.drawImage(canvas, 0, 0, W, H - step, 0, step, W, H - step);
    this.rowCtx.putImageData(img, 0, 0);
    ctx.drawImage(this.rowCanvas, 0, 0, W, step);
  }

  draw(vs: number, ve: number) {
    if (vs === this.lastVs && ve === this.lastVe) return;
    const { ctx, canvas } = this;
    const W = canvas.width;
    const H = canvas.height;
    if (this.lastVe > this.lastVs && W > 0 && H > 0) {
      // stretch and shift the history so it stays lined up with the new view
      const a = (this.lastVe - this.lastVs) / (ve - vs);
      const b = ((this.lastVs - vs) / (ve - vs)) * W;
      const tmp = document.createElement('canvas');
      tmp.width = W;
      tmp.height = H;
      tmp.getContext('2d')!.drawImage(canvas, 0, 0);
      this.fill(0, 0, W, H);
      ctx.drawImage(tmp, 0, 0, W, H, b, 0, a * W, H);
    }
    this.lastVs = vs;
    this.lastVe = ve;
  }
}
