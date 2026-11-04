// WebGL waterfall. Rows live in a GPU texture ring in capture coordinates (one texel per FFT bin, each row's capture
// centre stored beside it) and the shader maps the view onto them. A new row is one small texture upload; zooming and
// panning change two uniforms; nothing is copied or re-uploaded per frame, which is what the 2D canvas version spent
// most of its time on.
import type { Renderer } from './renderer';

/** Rows of history kept. */
export const ROWS = 1024;
/** Row centres are stored as 16-bit bin offsets from a reference centre; past this the ring is cleared and re-based. */
const MAX_OFFSET_BINS = 32_000;

const VERT = `
attribute vec2 p;
varying vec2 uv;
void main() {
  uv = vec2(p.x, 1.0 - p.y);
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const FRAG = `
precision highp float;
varying vec2 uv;
uniform sampler2D hist;
uniform sampler2D cen;
uniform sampler2D lut;
uniform float rows;
uniform float write;
uniform float heightPx;
uniform float rowPx;
uniform float vsb;
uniform float spanb;
uniform float n;
uniform vec3 bg;
void main() {
  // screen row, 0 = newest, and where it sits in the ring
  float r = floor(uv.y * heightPx / rowPx);
  float t = mod(write - 1.0 - r, rows);
  float v = (t + 0.5) / rows;
  vec4 c = texture2D(cen, vec2(0.5, v));
  if (r >= rows || c.b < 0.5) {
    gl_FragColor = vec4(bg, 1.0);
    return;
  }
  // the row's capture centre and this pixel's frequency, both in bins from the reference centre
  float off = floor(c.r * 255.0 + 0.5) * 256.0 + floor(c.g * 255.0 + 0.5) - 32768.0;
  float fb = vsb + uv.x * spanb;
  float u = (fb - off + 0.5) / n + 0.5; // bin k is centred on texel k + n/2
  if (u < 0.0 || u > 1.0) {
    gl_FragColor = vec4(bg, 1.0);
    return;
  }
  float level = texture2D(hist, vec2(u, v)).r;
  gl_FragColor = vec4(texture2D(lut, vec2(level, 0.5)).rgb, 1.0);
}`;

/** A row centre as the two bytes the shader reads back. */
export function encodeOffset(bins: number): [number, number] {
  const k = Math.round(bins) + 32768;
  return [(k >> 8) & 255, k & 255];
}

export class GlWaterfall implements Renderer {
  private gl: WebGLRenderingContext;
  private prog!: WebGLProgram;
  private hist: WebGLTexture | null = null;
  private cen!: WebGLTexture;
  private lut!: WebGLTexture;
  private u: Record<string, WebGLUniformLocation | null> = {};
  private n = 0;
  private write = 0;
  private refCenter: number | null = null;
  private rate = 1;
  private rowPx = 1;
  private bg: [number, number, number] = [0, 0, 0];
  private palette: Uint8ClampedArray | null = null;
  private lost = false;
  private readonly cenRow = new Uint8Array([0, 0, 255, 255]);

  /** Throws when WebGL is not available. */
  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl', { alpha: false, antialias: false, preserveDrawingBuffer: false, depth: false, stencil: false });
    if (!gl) throw new Error('no WebGL');
    this.gl = gl;
    canvas.addEventListener('webglcontextlost', (e) => {
      e.preventDefault();
      this.lost = true;
    });
    canvas.addEventListener('webglcontextrestored', () => {
      this.lost = false;
      this.setup();
    });
    this.setup();
  }

  private setup() {
    const gl = this.gl;
    const shader = (type: number, src: string) => {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'shader');
      return s;
    };
    const prog = gl.createProgram()!;
    gl.attachShader(prog, shader(gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, shader(gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog) ?? 'link');
    gl.useProgram(prog);
    this.prog = prog;
    for (const name of ['hist', 'cen', 'lut', 'rows', 'write', 'heightPx', 'rowPx', 'vsb', 'spanb', 'n', 'bg']) this.u[name] = gl.getUniformLocation(prog, name);
    const quad = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
    const p = gl.getAttribLocation(prog, 'p');
    gl.enableVertexAttribArray(p);
    gl.vertexAttribPointer(p, 2, gl.FLOAT, false, 0, 0);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    this.cen = this.texture(1, gl.RGBA, 1, ROWS, gl.NEAREST);
    this.lut = this.texture(2, gl.RGB, 256, 1, gl.LINEAR);
    gl.uniform1i(this.u.hist, 0);
    gl.uniform1i(this.u.cen, 1);
    gl.uniform1i(this.u.lut, 2);
    gl.uniform1f(this.u.rows, ROWS);
    this.hist = null;
    this.n = 0;
    this.write = 0;
    this.refCenter = null;
    if (this.palette) this.setPalette(this.palette);
  }

  private texture(unit: number, format: number, w: number, h: number, filter: number) {
    const gl = this.gl;
    const t = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, t);
    gl.texImage2D(gl.TEXTURE_2D, 0, format, w, h, 0, format, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return t;
  }

  resize(w: number, h: number, rowPx: number) {
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.rowPx = rowPx;
  }

  setPalette(lut: Uint8ClampedArray) {
    this.palette = lut;
    this.bg = [lut[0] / 255, lut[1] / 255, lut[2] / 255];
    if (this.lost) return;
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, this.lut);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, 256, 1, gl.RGB, gl.UNSIGNED_BYTE, new Uint8Array(lut.buffer, lut.byteOffset, 256 * 3));
  }

  /** Forget the history: a new band has nothing in common with the old rows. */
  clear() {
    if (this.lost) return;
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.cen);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, 1, ROWS, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(ROWS * 4));
    this.write = 0;
    this.refCenter = null;
  }

  push(bins: Uint8Array, centerHz: number, rate: number) {
    if (this.lost) return;
    const gl = this.gl;
    if (this.n !== bins.length) {
      if (this.hist) gl.deleteTexture(this.hist);
      this.hist = this.texture(0, gl.LUMINANCE, bins.length, ROWS, gl.LINEAR);
      this.n = bins.length;
      this.clear();
    }
    this.rate = rate;
    const binHz = rate / this.n;
    if (this.refCenter === null || Math.abs(centerHz - this.refCenter) / binHz > MAX_OFFSET_BINS) {
      this.clear();
      this.refCenter = centerHz;
    }
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.hist);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, this.write, this.n, 1, gl.LUMINANCE, gl.UNSIGNED_BYTE, bins);
    const [hi, lo] = encodeOffset((centerHz - this.refCenter) / binHz);
    this.cenRow[0] = hi;
    this.cenRow[1] = lo;
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, this.cen);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, this.write, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, this.cenRow);
    this.write = (this.write + 1) % ROWS;
  }

  draw(vs: number, ve: number) {
    if (this.lost) return;
    const gl = this.gl;
    const { width, height } = this.canvas;
    gl.viewport(0, 0, width, height);
    const binHz = this.n ? this.rate / this.n : 1;
    const ref = this.refCenter ?? vs;
    gl.uniform1f(this.u.write, this.write);
    gl.uniform1f(this.u.heightPx, height);
    gl.uniform1f(this.u.rowPx, this.rowPx);
    gl.uniform1f(this.u.vsb, (vs - ref) / binHz);
    gl.uniform1f(this.u.spanb, (ve - vs) / binHz);
    gl.uniform1f(this.u.n, this.n || 1);
    gl.uniform3f(this.u.bg, this.bg[0], this.bg[1], this.bg[2]);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }
}
