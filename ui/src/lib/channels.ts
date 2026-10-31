// Fixed channel plans for bands where radios pick numbered channels, keyed by band id.
// Sources (checked 2026-10-30): CB 47 CFR 95.963, FRS 47 CFR 95.563, NOAA weather.gov/nwr,
// marine RadioReference "Marine VHF Channels" (US), railroad AAR 007–097 at 15 kHz from 160.215.

export interface Channel {
  hz: number;
  name: string;
}

const CB = [26.965, 26.975, 26.985, 27.005, 27.015, 27.025, 27.035, 27.055, 27.065, 27.075, 27.085, 27.105, 27.115, 27.125, 27.135, 27.155, 27.165, 27.175, 27.185, 27.205, 27.215, 27.225, 27.255, 27.235, 27.245, 27.265, 27.275, 27.285, 27.295, 27.305, 27.315, 27.325, 27.335, 27.345, 27.355, 27.365, 27.375, 27.385, 27.395, 27.405];

const FRS = [462.5625, 462.5875, 462.6125, 462.6375, 462.6625, 462.6875, 462.7125, 467.5625, 467.5875, 467.6125, 467.6375, 467.6625, 467.6875, 467.7125, 462.55, 462.575, 462.6, 462.625, 462.65, 462.675, 462.7, 462.725];

// [channel, ship transmit, ship receive]; duplex channels are heard on both
const MARINE: [string, number, number][] = [
  ['1A', 156.05, 156.05], ['5A', 156.25, 156.25], ['6', 156.3, 156.3], ['7A', 156.35, 156.35], ['8', 156.4, 156.4],
  ['9', 156.45, 156.45], ['10', 156.5, 156.5], ['11', 156.55, 156.55], ['12', 156.6, 156.6], ['13', 156.65, 156.65],
  ['14', 156.7, 156.7], ['15', 156.75, 156.75], ['16', 156.8, 156.8], ['17', 156.85, 156.85], ['18A', 156.9, 156.9],
  ['19A', 156.95, 156.95], ['20', 157.0, 161.6], ['21A', 157.05, 157.05], ['22A', 157.1, 157.1], ['23A', 157.15, 157.15],
  ['24', 157.2, 161.8], ['25', 157.25, 161.85], ['26', 157.3, 161.9], ['27', 157.35, 161.95], ['28', 157.4, 162.0],
  ['63A', 156.175, 156.175], ['65A', 156.275, 156.275], ['66A', 156.325, 156.325], ['67', 156.375, 156.375],
  ['68', 156.425, 156.425], ['69', 156.475, 156.475], ['70', 156.525, 156.525], ['71', 156.575, 156.575],
  ['72', 156.625, 156.625], ['73', 156.675, 156.675], ['74', 156.725, 156.725], ['77', 156.875, 156.875],
  ['78A', 156.925, 156.925], ['79A', 156.975, 156.975], ['80A', 157.025, 157.025], ['81A', 157.075, 157.075],
  ['82A', 157.125, 157.125], ['83A', 157.175, 157.175], ['84', 157.225, 161.825], ['85', 157.275, 161.875],
  ['86', 157.325, 161.925], ['87', 157.375, 157.375], ['88', 157.425, 157.425],
];

const mhz = (m: number) => Math.round(m * 1e6);

export const CHANNELS: Record<string, Channel[]> = {
  cb: CB.map((m, i) => ({ hz: mhz(m), name: `CB ${i + 1}` })),
  frs: FRS.map((m, i) => ({ hz: mhz(m), name: `FRS ${i + 1}` })),
  wx: [162.4, 162.425, 162.45, 162.475, 162.5, 162.525, 162.55].map((m, i) => ({ hz: mhz(m), name: `WX${i + 1}` })),
  marine: MARINE.flatMap(([ch, tx, rx]) => {
    const name = `Marine ${ch}`;
    return tx === rx ? [{ hz: mhz(tx), name }] : [{ hz: mhz(rx), name: `${name} shore` }, { hz: mhz(tx), name: `${name} ship` }];
  }),
  rail: Array.from({ length: 91 }, (_, i) => ({ hz: mhz(160.215) + i * 15_000, name: `AAR ${String(i + 7).padStart(3, '0')}` })),
};

/** The channel nearest `hz` in a band's plan, if within half the plan's tightest spacing. */
export function channelAt(bandId: string | undefined, hz: number): Channel | undefined {
  const plan = bandId ? CHANNELS[bandId] : undefined;
  if (!plan) return undefined;
  let best: Channel | undefined;
  for (const c of plan) if (!best || Math.abs(c.hz - hz) < Math.abs(best.hz - hz)) best = c;
  return best && Math.abs(best.hz - hz) <= halfStep(plan) ? best : undefined;
}

/** Minimum gap between channel numbers on the frequency scale, in pixels. */
export const CHANNEL_LABEL_PX = 30;

export interface ChannelTick {
  left: number;
  text: string | null;
}

/**
 * Channel marks for the frequency scale between `vs` and `ve`, as % from the left. Every
 * channel gets a mark; numbers are thinned so they sit at least CHANNEL_LABEL_PX apart.
 */
export function channelTicks(bandId: string | undefined, vs: number, ve: number, widthPx: number): ChannelTick[] {
  const plan = bandId ? CHANNELS[bandId] : undefined;
  if (!plan || widthPx <= 0) return [];
  const span = ve - vs;
  const inView = plan.filter((c) => c.hz > vs && c.hz < ve).sort((a, b) => a.hz - b.hz);
  let lastPx = -Infinity;
  return inView.map((c) => {
    const px = ((c.hz - vs) / span) * widthPx;
    const fits = px - lastPx >= CHANNEL_LABEL_PX && px > 12 && px < widthPx - 12;
    if (fits) lastPx = px;
    return { left: (px / widthPx) * 100, text: fits ? c.name.replace(/^\S+ /, '') : null };
  });
}

const steps = new WeakMap<Channel[], number>();
function halfStep(plan: Channel[]): number {
  let s = steps.get(plan);
  if (s === undefined) {
    const hz = plan.map((c) => c.hz).sort((a, b) => a - b);
    s = Infinity;
    for (let i = 1; i < hz.length; i++) if (hz[i] > hz[i - 1]) s = Math.min(s, hz[i] - hz[i - 1]);
    s /= 2;
    steps.set(plan, s);
  }
  return s;
}
