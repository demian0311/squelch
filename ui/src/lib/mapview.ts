// A map's view: where it looks and how far, in km from the listener, and the street-map gestures that move it.
// The Repeaters and Airplanes maps share this; each gives its own limits.

/** Where the map looks: its centre in km east and south of the listener, and how far it shows above and below the centre. */
export interface MapView {
  x: number;
  y: number;
  halfKm: number;
}

/** How close and how wide a map goes, in km above and below the centre. */
export interface MapLimits {
  nearestKm: number;
  widestKm: number;
}

/** The view centred on the listener at this half-height. */
export const viewOf = (halfKm: number): MapView => ({ x: 0, y: 0, halfKm });

/** Never closer than `nearestKm`, never wider than `widestKm`, and the edge never leaves the widest view: no bare ground past it. */
export function clampView(v: MapView, lim: MapLimits): MapView {
  const halfKm = Math.min(lim.widestKm, Math.max(lim.nearestKm, v.halfKm));
  const edge = lim.widestKm - halfKm;
  // `|| 0` keeps a -0 out of the view
  return { x: Math.min(edge, Math.max(-edge, v.x)) || 0, y: Math.min(edge, Math.max(-edge, v.y)) || 0, halfKm };
}

/**
 * The view zoomed by `factor` (under 1 = closer) so that the place `at` (km) stays where it was on screen,
 * the way a street map zooms around the pointer. Without `at` it zooms around the centre.
 */
export function zoomView(v: MapView, factor: number, lim: MapLimits, at?: { x: number; y: number }): MapView {
  const halfKm = Math.min(lim.widestKm, Math.max(lim.nearestKm, v.halfKm * factor));
  const f = halfKm / v.halfKm;
  const a = at ?? { x: v.x, y: v.y };
  return clampView({ x: a.x + (v.x - a.x) * f, y: a.y + (v.y - a.y) * f, halfKm }, lim);
}

/** Distance rings to choose from, km. */
export const RING_STEPS_KM = [1, 2, 5, 10, 20, 25, 50, 100, 200, 400];
/** The widest ring that fits the view: `share` of its half-height at most. */
export function ringKm(halfKm: number, share = 1, steps = RING_STEPS_KM): number {
  return steps.filter((d) => d <= halfKm * share).at(-1) ?? steps[0];
}

/** A wheel notch zooms this much; a line-mode wheel reports lines, not pixels. */
export const WHEEL_STEP = 0.0015;
export const LINE_PX = 16;
/** One press of + or −. */
export const BUTTON_STEP = 0.6;
/** A press that moves less than this is a click, not a drag. */
const DRAG_PX = 3;

export interface GestureTarget {
  view: () => MapView;
  kmPerPx: () => number;
  svg: () => SVGSVGElement | null | undefined;
  limits: () => MapLimits;
  onView: (v: MapView) => void;
  /** a drag started or ended: the cursor changes */
  onDragging?: (dragging: boolean) => void;
}

/**
 * Zoom and pan like a street map: scroll zooms around the pointer, drag pans, double-click zooms in.
 * A press is captured only once it is a drag, so a plain press still clicks the mark under it;
 * `dragged()` is true right after a drag, so the click that follows one is not a pick.
 */
export function mapGestures(t: GestureTarget) {
  let drag: { id: number; x: number; y: number; view: MapView; moved: boolean } | null = null;
  let dragged = false;
  function kmAt(e: { clientX: number; clientY: number }) {
    const r = t.svg()!.getBoundingClientRect();
    const v = t.view();
    const k = t.kmPerPx();
    return { x: v.x + (e.clientX - r.left - r.width / 2) * k, y: v.y + (e.clientY - r.top - r.height / 2) * k };
  }
  return {
    kmAt,
    dragged: () => dragged,
    onwheel(e: WheelEvent) {
      e.preventDefault();
      const dy = e.deltaMode === 1 ? e.deltaY * LINE_PX : e.deltaY;
      t.onView(zoomView(t.view(), Math.exp(dy * WHEEL_STEP), t.limits(), kmAt(e)));
    },
    onpointerdown(e: PointerEvent) {
      if (e.button !== 0) return;
      drag = { id: e.pointerId, x: e.clientX, y: e.clientY, view: t.view(), moved: false };
    },
    onpointermove(e: PointerEvent) {
      if (!drag) return;
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      if (!drag.moved && Math.hypot(dx, dy) < DRAG_PX) return;
      if (!drag.moved) {
        t.svg()?.setPointerCapture(drag.id);
        t.onDragging?.(true);
      }
      drag.moved = true;
      const k = t.kmPerPx();
      t.onView(clampView({ ...drag.view, x: drag.view.x - dx * k, y: drag.view.y - dy * k }, t.limits()));
    },
    onpointerup() {
      dragged = !!drag?.moved;
      drag = null;
      t.onDragging?.(false);
    },
    ondblclick(e: MouseEvent) {
      t.onView(zoomView(t.view(), 0.5, t.limits(), kmAt(e)));
    },
  };
}
