import { describe, expect, it } from 'vitest';
import type { AcarsMessage, Aircraft, Airport, BandPlan } from './api';
import { planeKind } from './adsb';
import { airportLabel, airportShort, airportsFor, hearingSay, lookupLinks, flightOf, friendlyCallsign, isFrom, messageReading, messageSender, messageTitle, oneAirplanes, placeLabels, positionsIn, routeIn, routeText, talkGuess } from './airplanes';

const plane = (o: Partial<Aircraft> = {}): Aircraft => ({ icao: 'A00001', callsign: null, alt_ft: null, speed_kt: null, heading_deg: null, airspeed: false, vrate_fpm: null, lat: null, lon: null, age_s: 0, pos_age_s: 0, messages: 1, snr_db: 10, registration: null, country: null, category: null, squawk: null, emergency: null, advisory: false, alert: false, spi: false, on_ground: null, gnss_alt_ft: null, selected_alt_ft: null, selected_alt_fms: false, selected_heading_deg: null, baro_hpa: null, modes: [], adsb_version: null, ...o } as Aircraft);
const msg = (o: Partial<AcarsMessage>): AcarsMessage => ({ id: 1, at_ms: 1, hz: 1, mode: '2', registration: 'N1', ack: null, label: 'H1', block_id: '1', more: false, msn: 'M01A', flight: 'XX1', text: '', ...o });
const MIA: Airport = { ident: 'KMIA', iata: 'MIA', size: 'L', lat: 25.796, lon: -80.29, name: 'Miami International Airport', city: 'Miami', channels: [{ kind: 'APP', mhz: 120.5, name: 'APP' }, { kind: 'DEP', mhz: 119.45, name: 'DEP' }, { kind: 'GND', mhz: 121.8, name: 'GND' }, { kind: 'TWR', mhz: 118.3, name: 'TWR' }] };
const FLL: Airport = { ident: 'KFLL', iata: 'FLL', size: 'L', lat: 26.0726, lon: -80.1527, name: 'Fort Lauderdale', city: 'Fort Lauderdale', channels: [{ kind: 'APP', mhz: 118.1, name: 'MIAMI APP' }, { kind: 'TWR', mhz: 119.3, name: 'TWR' }] };
const OPF: Airport = { ident: 'KOPF', iata: 'OPF', size: 'M', lat: 25.907, lon: -80.278, name: 'Opa-locka', city: 'Miami', channels: [{ kind: 'TWR', mhz: 120.7, name: 'TWR' }] };
const SMALL: Airport = { ident: 'X1', iata: '', size: 'S', lat: 25.9, lon: -80.28, name: 'Strip', city: 'Nowhere', channels: [{ kind: 'CTAF', mhz: 122.8, name: 'CTAF' }] };

describe('names', () => {
  it('turns airline callsigns into words and leaves the rest', () => {
    expect(friendlyCallsign('AAL2981')).toBe('American 2981');
    expect(friendlyCallsign('N251V')).toBe('N251V');
    expect(friendlyCallsign('QZX12')).toBe('QZX12');
  });
  it('names a message by its airline', () => {
    expect(messageSender({ flight: 'AA0213', registration: 'N1' })).toBe('American 213');
    expect(messageSender({ flight: '', registration: 'N1' })).toBe('N1');
  });
});

describe('matching a message to a plane', () => {
  it('goes by flight number, across the two code systems', () => {
    expect(flightOf('AAL2981')).toBe('AA2981');
    expect(isFrom(msg({ flight: 'AA2981', registration: 'N966AN' }), plane({ callsign: 'AAL2981' }))).toBe(true);
    expect(isFrom(msg({ flight: 'F90214' }), plane({ callsign: 'FFT214' }))).toBe(true);
    expect(isFrom(msg({ flight: 'AA2981' }), plane({ callsign: 'AAL298' }))).toBe(false);
  });
  it('goes by tail number, whatever the punctuation', () => {
    expect(isFrom(msg({ registration: '.N966AN' }), plane({ registration: 'N966AN' }))).toBe(true);
    expect(isFrom(msg({ registration: 'A6-EWH' }), plane({ registration: 'A6EWH' }))).toBe(true);
  });
});

describe('reading messages', () => {
  it('finds routes in the layouts heard on air', () => {
    expect(routeIn('40 IV3 FPR\n. FLT 1018 MIA-MCO SKD ARR')).toEqual({ from: 'MIA', to: 'MCO' });
    expect(routeIn('#DFBD3MF03STL FLL N25498W079598')).toEqual({ from: 'STL', to: 'FLL' });
    expect(routeIn('#DFB/WX02EN03KFLLKATL\nN26106')).toEqual({ from: 'KFLL', to: 'KATL' });
    expect(routeIn('#DFBA04/A32104,1,1/CCN568UW,NOV03,135654,KMIA,KFLL,0518')).toEqual({ from: 'KMIA', to: 'KFLL' });
    expect(routeIn('#CFBWRN/WN26110313590022000006AUTO FLT AP OFF')).toBe(null);
  });
  it('reads positions with a time after them, across a line break', () => {
    const text = '#DFBD3MF03STL FLL N25498W07959813440593P140102012G0009N25520W079589134\n40547P15';
    const p = positionsIn({ label: 'H1', text });
    expect(p.length).toBe(2);
    expect(p[0].lat).toBeCloseTo(25.83, 2);
    expect(p[0].lon).toBeCloseTo(-79.9967, 3);
  });
  it('does not take flight plan route points for positions', () => {
    expect(positionsIn({ label: 'H1', text: '#M1BREQPWI/WQ240.210.180:N26156W080228.ALNZO.FLMGO.N31312W089548.FESHA' })).toEqual([]);
  });
  it('uses the documented layouts too', () => {
    expect(positionsIn({ label: '21', text: 'POSN 26.082W 80.075, 229' })).toEqual([{ lat: 26.082, lon: -80.075 }]);
  });
  it('says what a message is only where the text spells it out', () => {
    expect(messageTitle({ label: '21', text: 'POSN 26.082W 80.075, 229' })).toBe('Reported where it was');
    expect(messageTitle({ label: 'H1', text: '#CFBWRN/WN26110313590022000006AUTO FLT AP OFF' })).toBe('Autopilot off');
    expect(messageTitle({ label: 'H1', text: '#DFB2 0002' })).toBe('Aircraft system numbers');
    expect(messageTitle({ label: 'ZZ', text: 'x' })).toBe('Airline message');
  });
});

describe('who a plane is probably talking to', () => {
  const airports = [OPF, SMALL, MIA, FLL];
  it('sends an airliner to the big airport, not a nearer small one', () => {
    const t = talkGuess(plane({ lat: 25.9, lon: -80.28, alt_ft: 5000, vrate_fpm: -800, category: 'Large' }), airports, null);
    expect(t?.airport.ident).toBe('KMIA');
    expect(t?.channel.mhz).toBe(120.5);
    expect(t?.name).toBe('Miami Approach');
  });
  it('believes a destination from the plane\'s own message', () => {
    const t = talkGuess(plane({ lat: 25.7, lon: -80.3, alt_ft: 3925, category: 'Large' }), airports, 'FLL');
    expect(t?.airport.ident).toBe('KFLL');
    expect(t?.name).toBe('Miami Approach');
    expect(t?.why).toMatch(/message/);
  });
  it('gives the tower to a low plane close in, departure to a climbing one', () => {
    expect(talkGuess(plane({ lat: 25.8, lon: -80.28, alt_ft: 900, category: 'Large' }), airports, null)?.channel.kind).toBe('TWR');
    // a tower is one airport's, so it carries that airport's name, not the city's: Miami has three towers
    expect(talkGuess(plane({ lat: 25.908, lon: -80.278, alt_ft: 900, category: 'Light' }), airports, null)?.name).toBe('Opa-locka Tower');
    expect(talkGuess(plane({ lat: 25.8, lon: -80.28, alt_ft: 900, category: 'Large' }), airports, null)?.name).toBe('Miami International Tower');
    expect(talkGuess(plane({ lat: 25.9, lon: -80.5, alt_ft: 6000, vrate_fpm: 1500, category: 'Large' }), airports, null)?.channel.kind).toBe('DEP');
    expect(talkGuess(plane({ lat: 25.796, lon: -80.29, on_ground: true, alt_ft: 0, category: 'Large' }), airports, null)?.channel.kind).toBe('GND');
  });
  it('lets a small plane use a small field', () => {
    expect(talkGuess(plane({ lat: 25.9, lon: -80.28, alt_ft: 800, category: 'Light' }), airports, null)?.airport.ident).toBe('X1');
  });
  it('says nothing when it cannot know', () => {
    expect(talkGuess(plane({ lat: 25.8, lon: -80.3, alt_ft: 35000 }), airports, null)).toBe(null);
    expect(talkGuess(plane({ alt_ft: 3000 }), airports, null)).toBe(null);
    expect(talkGuess(plane({ lat: 40, lon: -100, alt_ft: 3000 }), airports, null)).toBe(null);
  });
});

describe('the map', () => {
  it('draws small fields only when zoomed in', () => {
    const all = [MIA, OPF, SMALL];
    expect(airportsFor(all, 50).length).toBe(3);
    expect(airportsFor(all, 100).map((a) => a.ident)).toEqual(['KMIA', 'KOPF']);
    expect(airportsFor(all, 300).map((a) => a.ident)).toEqual(['KMIA']);
  });
  it('writes a route in cities, or codes when unknown', () => {
    expect(routeText({ from: 'KMIA', to: 'ZZZ' }, [MIA])).toBe('Miami → ZZZ');
  });
  it('moves a label to a free side, and drops it when none is free', () => {
    const items = [{ id: 'a', x: 0, y: 0, text: 'AAAA' }, { id: 'b', x: 0, y: 0, text: 'BBBB' }, { id: 'c', x: 0, y: 0, text: 'CCCC' }, { id: 'd', x: 0, y: 0, text: 'DDDD' }, { id: 'e', x: 0, y: 0, text: 'EEEE' }];
    const p = placeLabels(items, 1);
    expect(p.get('a')?.side).toBe('right');
    expect(p.get('b')?.side).toBe('left');
    expect(p.get('c')?.side).toBe('above');
    expect(p.get('d')?.side).toBe('below');
    expect(p.has('e')).toBe(false);
  });
  it('keeps labels off symbols and fixed text', () => {
    // a plane 12 px right of an airport ring: right of the plane is free, but left would cross the ring
    const p = placeLabels([{ id: 'ring', x: 0, y: 0, text: 'MIA', r: 6 }, { id: 'p', x: 12, y: 0, text: 'AAL1', r: 10 }], 1);
    expect(p.get('ring')?.side).toBe('left');
    expect(p.get('p')?.side).toBe('right');
    expect(p.get('p')?.x).toBe(24);
    // text already drawn takes its room and gets no entry
    const q = placeLabels([{ id: 'km', x: 10, y: -6, text: '50 km', fixed: true }, { id: 'p', x: 0, y: 0, text: 'AAL1' }], 1);
    expect(q.has('km')).toBe(false);
    expect(q.get('p')?.side).toBe('left');
    // when the four sides are taken, a corner will do
    const five = ['a', 'b', 'c', 'd', 'e'].map((id) => ({ id, x: 0, y: 0, text: 'XX' }));
    expect(placeLabels(five, 1).get('e')?.side).toBe('right-above');
  });
  it('folds the aircraft bands into one entry', () => {
    const b = (id: string, decoder?: string) => ({ id, group: 'g', name: id, icon: 'x', mode: 'am', low_mhz: 1, high_mhz: 2, start_mhz: 1, decoder, bookmarks: [] });
    const plan = { groups: [{ id: 'g', name: 'G' }], bands: [b('air'), b('adsb', 'adsb'), b('acars', 'acars'), b('fm')] } as unknown as BandPlan;
    expect(oneAirplanes(plan).bands.map((x) => [x.id, x.name])).toEqual([['air', 'Airplanes'], ['fm', 'fm']]);
  });
});

describe('planeKind', () => {
  it('says what the plane is in everyday words', () => {
    expect(planeKind('High vortex')).toBe('Large plane');
    expect(planeKind('Medium')).toBe('Medium plane');
    expect(planeKind('Helicopter')).toBe('Helicopter');
    expect(planeKind(null)).toBe('');
  });
});

describe('lookupLinks', () => {
  it('offers the FAA only for a US tail, and the live map always', () => {
    expect(lookupLinks('A9C508', 'N729AN').map((l) => l.label)).toEqual(['FlightAware', 'FAA record', 'Live map']);
    expect(lookupLinks('4CA123', 'EI-ABC').map((l) => l.label)).toEqual(['FlightAware', 'Live map']);
    expect(lookupLinks('A9C508', null)).toEqual([{ label: 'Live map', url: 'https://globe.adsbexchange.com/?icao=a9c508' }]);
  });
});

describe('airportLabel', () => {
  const a = { ident: 'KMIA', iata: 'MIA', size: 'L', city: 'Miami' } as Airport;
  it('names a big airport by its city', () => {
    expect(airportLabel(a)).toBe('Miami · MIA');
    expect(airportLabel({ ...a, iata: 'OPF', size: 'M' })).toBe('OPF');
  });
});

describe('messageReading, on messages heard 2026-11-03', () => {
  const read = (label: string, text: string, known: Airport[] = []) => messageReading({ label, text }, known);
  const fll = { ident: 'KFLL', iata: 'FLL', city: 'Fort Lauderdale' } as Airport;

  it('says what went wrong in a fault report', () => {
    const r = read('H1', '#CFBFLR/FR26110313560030310005SDAC1:NO DATA FROM  PHC2\n/IDECAM 2');
    expect(r.title).toBe('Fault: No data from PHC2');
    expect(r.facts).toEqual([{ label: 'When', value: '13:56 UTC' }, { label: 'System', value: 'SDAC1' }]);
  });

  it('reads a cockpit warning and the autopilot going off', () => {
    expect(read('H1', '#CFBWRN/WN26110313590022000006AUTO FLT AP OFF').title).toBe('Autopilot off');
    expect(read('H1', '#CFBWRN/WN26110313590022000006ENG 1 FIRE').title).toMatch(/^Cockpit warning: /);
  });

  it('reads a radio link report by the published layout', () => {
    const r = read('SA', '0EV135433VS');
    expect(r.title).toBe('Radio link connected: VHF radio');
    expect(r.facts).toEqual([{ label: 'When', value: '13:54 UTC' }, { label: 'Links working', value: 'VHF radio, satellite' }]);
  });

  it('reads where a plane is heading to land', () => {
    expect(read('H1', '#M1BPRG/DTKFLL,10R,119,143130,048/FNAAL518TB72F', [fll]).title).toBe('Heading for Fort Lauderdale, runway 10R');
  });

  it('lists route points without the coordinates', () => {
    const r = read('H1', '#M1BREQPWI/WQ240.210.180:N26156W080228.ALNZO.FLMGO.PRESK.PRESK/DQ200/WRKMCO.KTPAFCAA');
    expect(r.facts).toEqual([{ label: 'Route points', value: 'ALNZO FLMGO PRESK' }]);
  });

  it('finds the route in airline data and says the rest has no key', () => {
    const r = read('H1', '#10BSS0340F_343\nA6-EWH  UAE213OMDBKMIA031126141948AP\n146  574223');
    expect(r.title).toBe('Aircraft system numbers');
    expect(r.facts).toEqual([{ label: 'Route', value: 'OMDB → KMIA' }]);
    expect(r.note).toBeTruthy();
  });

  it('reads a departure message and how late a flight is', () => {
    const d = read('10', '10KF DEPMSG 0840/04 KMIA/PANC .N485MC\n/POS N25558W080066/FOB 0720\nTOF/ 0712\nSOB/ 003');
    expect(d.title).toBe('Departure message');
    expect(d.facts).toContainEqual({ label: 'People aboard', value: '3' });
    expect(d.facts).toContainEqual({ label: 'Route', value: 'KMIA → PANC' });
    expect(read('HX', '40 IV3 FPR\n. FLT 1018 MIA-MCO SKD ARR 0931L\n A PLUS 14 0945L').title).toBe('Flight schedule: 14 min late');
  });

  it('reads a message whose first block was missed', () => {
    expect(read('H1', '… #M1BFLOW ENG 2\n6101  PPH\nP30 BURNER PRESSURE ENG 2').title).toBe('Engine readings');
    expect(read('H1', '…\n298929762927 21 21 2111').title).toBe('Airline message');
  });

  it('owns up to what cannot be read', () => {
    expect(read('48', "0141_l~b_~EE'(nl(lAE(bll?bd*1CbHy1yb#@CsSAllsALl(Alb#@CsyH_s52Gsl'LbL?").title).toBe('Coded data');
    expect(read('H1', '#DFB19458,16725,08713,D3/N101,00,00').note).toBeTruthy();
  });
});

describe('what the channel strip says', () => {
  it('drops the word every airport shares', () => {
    expect(airportShort(MIA)).toBe('Miami International');
    expect(airportShort(OPF)).toBe('Opa-locka');
  });
  it('owns up to silence and counts calls', () => {
    expect(hearingSay({ talking: false, calls: 0, lastAt: null }, 1000)).toBe('Quiet so far');
    expect(hearingSay({ talking: true, calls: 2, lastAt: 900 }, 1000)).toBe('Someone is talking now');
    expect(hearingSay({ talking: false, calls: 1, lastAt: 1_000 }, 11_000)).toBe('1 call heard, the last 10 s ago');
    expect(hearingSay({ talking: false, calls: 3, lastAt: 0 }, 200_000)).toBe('3 calls heard, the last 3 min ago');
  });
});
