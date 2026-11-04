// The Airplanes screen's logic: one plane, with what is known about it from each source, in plain words.
import type { AcarsMessage, Aircraft, Airport, AirportChannel, BandPlan } from './api';
import { acarsPosition, labelName } from './acars';
import airlines from './airlines.json';
import { kmBetween } from './repeaters';

/** Seconds the dongle spends on each job while watching. */
export const AIR_TURNS = { planesS: 20, messagesS: 10 };

const AIRLINES = airlines as unknown as Record<string, [string, string]>;

/** The rail shows one entry for everything about aircraft, under the voice band's id. */
export function oneAirplanes(plan: BandPlan): BandPlan {
  const bands = plan.bands.filter((b) => b.decoder !== 'adsb' && b.decoder !== 'acars').map((b) => (b.id === 'air' ? { ...b, name: 'Airplanes', sub: 'Map, messages, pilots' } : b));
  return { ...plan, groups: plan.groups.filter((g) => bands.some((b) => b.group === g.id)), bands };
}

/** AAL2981 as "American 2981"; anything else as it is. */
export function friendlyCallsign(callsign: string): string {
  const m = callsign.match(/^([A-Z]{3})(\d.*)$/);
  const a = m && AIRLINES[m[1]];
  return a ? `${a[1]} ${m![2]}` : callsign;
}

export const planeName = (a: Aircraft) => (a.callsign ? friendlyCallsign(a.callsign) : (a.registration ?? 'Unknown plane'));

/** A message's flight number (AA2981, F90214) as the callsign's parts, to match a plane's. */
export function flightOf(callsign: string | null): string | null {
  const m = callsign?.match(/^([A-Z]{3})0*(\d.*)$/);
  const iata = m && AIRLINES[m[1]]?.[0];
  return iata ? `${iata}${m![2]}` : null;
}

const flightKey = (f: string) => f.slice(0, 2) + f.slice(2).replace(/^0+/, '');

/** A message's sender as a name: "American 2981" from AA2981 when the airline is known. */
const IATA_NAMES = new Map(Object.values(AIRLINES).filter((a) => a[0]).map((a) => [a[0], a[1]]));
export function messageSender(m: { flight: string; registration: string }): string {
  const name = m.flight && IATA_NAMES.get(m.flight.slice(0, 2));
  if (name && /^\d/.test(m.flight.slice(2))) return `${name} ${m.flight.slice(2).replace(/^0+/, '')}`;
  return m.flight || m.registration;
}

/** Same airframe by tail number, or same flight number from the airline's codes. */
export function isFrom(m: AcarsMessage, a: Aircraft): boolean {
  if (a.registration && m.registration.replace(/[.-]/g, '') === a.registration.replace(/[.-]/g, '')) return true;
  const f = flightOf(a.callsign);
  return !!f && !!m.flight && flightKey(m.flight) === flightKey(f);
}

/**
 * Where the plane says it goes. The layouts are read off messages on air 2026-11-03, none is published:
 * "FLT 1018 MIA-MCO", "#DFBD3MF03STL FLL", "/WX02EN03KFLLKATL", ",KMIA,KFLL,".
 */
export function routeIn(text: string): { from: string; to: string } | null {
  const r =
    text.match(/\bFLT \d+ ([A-Z]{3})-([A-Z]{3})\b/) ??
    text.match(/D3M\w{3}([A-Z]{3}) ([A-Z]{3})\b/) ??
    text.match(/\/WX\d+EN\d+(K[A-Z]{3})(K[A-Z]{3})/) ??
    text.match(/,(K[A-Z]{3}),(K[A-Z]{3}),/);
  return r ? { from: r[1], to: r[2] } : null;
}

export function routeOf(messages: AcarsMessage[]): { from: string; to: string } | null {
  for (const m of messages) {
    const r = routeIn(m.text);
    if (r) return r;
  }
  return null;
}

/**
 * Positions a message holds: the documented layouts, then the run N25498W079598 followed by a time (degrees, minutes
 * and tenths), read off H1 telemetry on air 2026-11-03. Route points in flight plans (#M1B) have no time after them.
 */
export function positionsIn(m: { label: string; text: string }): { lat: number; lon: number }[] {
  const one = acarsPosition(m);
  if (one) return [one];
  if (m.label !== 'H1' || !m.text.startsWith('#DFB')) return [];
  const out: { lat: number; lon: number }[] = [];
  for (const r of m.text.replace(/\s+/g, '').matchAll(/([NS])(\d{2})(\d{3})([EW])(\d{3})(\d{3})\d{6}/g)) {
    const lat = (+r[2] + +r[3] / 600) * (r[1] === 'S' ? -1 : 1);
    const lon = (+r[5] + +r[6] / 600) * (r[4] === 'W' ? -1 : 1);
    if (Math.abs(lat) <= 90 && Math.abs(lon) <= 180) out.push({ lat, lon });
  }
  return out;
}

/** What a message is, in plain words. Text is only interpreted where it spells the thing out. */
export function messageTitle(m: { label: string; text: string }): string {
  if (positionsIn(m).length) return 'Reported where it was';
  if (/\bFLT \d+ [A-Z]{3}-[A-Z]{3}\b/.test(m.text)) return 'Flight schedule';
  if (/AUTO FLT AP OFF/.test(m.text)) return 'Autopilot off';
  if (/AUTO FLT A\/THR OFF/.test(m.text)) return 'Autothrottle off';
  if (m.label === 'H1') return m.text ? 'Data from the plane' : 'Empty message';
  return labelName(m.label) || 'Data from the plane';
}

export interface Talk {
  airport: Airport;
  channel: AirportChannel;
  /** "Miami Approach". */
  name: string;
  why: string;
}

const KIND_WORD: Record<string, string> = { TWR: 'Tower', GND: 'Ground', APP: 'Approach', DEP: 'Departure', 'A/D': 'Approach', CTAF: 'traffic channel', UNIC: 'traffic channel' };
const titleCase = (s: string) => s.toLowerCase().replace(/\b[a-z]/g, (c) => c.toUpperCase());

/** "MIAMI APP" names who answers on a Fort Lauderdale channel; a bare "APP" says nothing, so the city does. */
function talkName(a: Airport, c: AirportChannel): string {
  const word = KIND_WORD[c.kind] ?? c.kind;
  const said = c.name.match(/^([A-Z][A-Z .'-]*?)\s+(APP|DEP|A\/D|APP\/DEP)\b/);
  return `${said ? titleCase(said[1]) : a.city || a.name} ${word}`;
}

/** Above this planes talk to en-route controllers, and which one depends on sectors the list does not hold. */
const TOO_HIGH_FT = 18000;
/** Low and this close is the tower's. */
const TOWER_KM = 12;
const TOWER_FT = 2500;
/** Airports past this are not who a plane is talking to. */
const NEAR_KM = 90;
const BIG_PLANE = /large|heavy|high.?vortex/i;

/** A guess: planes do not broadcast who they talk to. Null when there is nothing honest to say. */
export function talkGuess(a: Aircraft, airports: Airport[], to: string | null): Talk | null {
  if (a.lat === null || a.lon === null) return null;
  const at = { lat: a.lat, lon: a.lon };
  const ft = a.alt_ft ?? 0;
  if (ft > TOO_HIGH_FT) return null;
  const km = (x: Airport) => kmBetween(at, { lat: x.lat, lon: x.lon });
  const big = BIG_PLANE.test(a.category ?? '');
  const candidates = airports.filter((x) => km(x) < NEAR_KM && x.channels.length && (!big || x.size !== 'S'));
  const dest = to ? candidates.find((x) => x.ident === to || x.iata === to) : undefined;
  const home = dest ?? candidates.slice().sort((x, y) => (big ? rank(x) - rank(y) : 0) || km(x) - km(y))[0];
  if (!home) return null;
  const climbing = (a.vrate_fpm ?? 0) > 200;
  const place = home.city || home.name;
  const wants: [string, string][] =
    a.on_ground ? [['GND', `On the ground at ${place}`]]
    : ft < TOWER_FT && km(home) < TOWER_KM ? [['TWR', `Low and close to ${place}`], ['CTAF', `Low and close to ${place}`], ['UNIC', `Low and close to ${place}`]]
    : climbing ? [['DEP', `Climbing away from ${place}`], ['A/D', `Climbing away from ${place}`], ['APP', `Climbing away from ${place}`]]
    : [['APP', dest ? `Its message says it is flying to ${place}` : `Nearing ${place}`], ['A/D', dest ? `Its message says it is flying to ${place}` : `Nearing ${place}`], ['TWR', `Close to ${place}`], ['CTAF', `Close to ${place}`]];
  for (const [kind, why] of wants) {
    const channel = home.channels.find((c) => c.kind === kind);
    if (channel) return { airport: home, channel, name: talkName(home, channel), why };
  }
  return null;
}
const rank = (x: Airport) => (x.size === 'L' ? 0 : x.size === 'M' ? 1 : 2);

/** Airports worth drawing at this map size: small fields only once zoomed in. */
export function airportsFor(airports: Airport[], halfKm: number): Airport[] {
  const sizes = halfKm <= 60 ? 'LMS' : halfKm <= 150 ? 'LM' : 'L';
  return airports.filter((a) => sizes.includes(a.size));
}

/** "St. Louis → Fort Lauderdale" with the airports' cities, or their codes when unknown. */
export function routeText(r: { from: string; to: string }, known: Airport[]): string {
  const city = (c: string) => {
    const a = known.find((x) => x.ident === c || x.iata === c);
    return a?.city || a?.name || c;
  };
  return `${city(r.from)} → ${city(r.to)}`;
}

export type LabelSide = 'right' | 'left' | 'above' | 'below';
interface LabelItem {
  id: string;
  x: number;
  y: number;
  text: string;
}

/**
 * Where each label fits without covering another, first come first placed, so put what matters first. Sizes are in the
 * caller's units per on-screen pixel. A label with no room gets no entry; its plane still has its arrow and its row.
 */
export function placeLabels(items: LabelItem[], unit: number, charPx = 6.2, heightPx = 13, offsetPx = 9): Map<string, { side: LabelSide; x: number; y: number }> {
  const out = new Map<string, { side: LabelSide; x: number; y: number }>();
  const taken: [number, number, number, number][] = [];
  const hits = (b: [number, number, number, number]) => taken.some((t) => b[0] < t[2] && b[2] > t[0] && b[1] < t[3] && b[3] > t[1]);
  for (const it of items) {
    const w = it.text.length * charPx * unit;
    const h = heightPx * unit;
    const off = offsetPx * unit;
    const tries: [LabelSide, number, number][] = [
      ['right', it.x + off, it.y - h / 2],
      ['left', it.x - off - w, it.y - h / 2],
      ['above', it.x - w / 2, it.y - off - h],
      ['below', it.x - w / 2, it.y + off],
    ];
    for (const [side, x, y] of tries) {
      const box: [number, number, number, number] = [x, y, x + w, y + h];
      if (!hits(box)) {
        taken.push(box);
        out.set(it.id, { side, x, y: y + h * 0.78 });
        break;
      }
    }
  }
  return out;
}
