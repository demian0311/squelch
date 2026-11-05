# Airplanes

Every airliner says where it is on 1090 MHz and texts its airline on 130 MHz. This screen draws the map from those. Pilots and controllers talk in AM between 118 and 137 MHz.

## What you will hear

- The map and the plane list are data, not sound: nobody talks on 1090 MHz.
- Messages (ACARS) read like telegrams: gate, weather, delays, position reports, engine faults. Near a big airport a few a minute.
- Pilots and controllers talk on the Aircraft voice channels in AM. Short, formal, and in English everywhere in the world. A plane's card guesses which channel from the nearest big airport and its height.

## How this radio reads it

ADS-B (Automatic Dependent Surveillance–Broadcast): each plane takes its position from GPS and broadcasts it, with height, speed and callsign, about twice a second on 1090 MHz. Nothing asks it to and nothing answers. With an antenna that sees the sky, 100 miles or more.

![A plane tells everyone where it is](diagram/adsb)

ACARS (Aircraft Communications Addressing and Reporting System): 2400 bit/s bursts on a handful of VHF channels; 131.550 MHz is the busiest in North America. The dongle hears one 2.4 MHz slice at a time, so while this screen is open it takes turns between 1090 MHz and 130 MHz; a Listen on a voice channel stops the turns.

Voice is AM, as aviation has always been: two stations at once both come through, so a controller hears that someone stepped on a call.

## Band plan

| MHz | What |
|---|---|
| 1090 | ADS-B and Mode S replies |
| 978 | UAT, small planes in the US (not heard here) |
| 129.125–131.550 | ACARS, six channels; 131.550 and 130.025 busiest |
| 121.5 | Emergency guard: every plane listens |
| 121.6–121.975 | Ground control |
| 122.7–123.05 | Small airports (UNICOM, CTAF) |
| 118–137 | Tower, approach, departure, centre |

![A flight's radio chain, with the usual frequency ranges](diagram/atc-chain)

## Rules

- Listening needs nothing: ADS-B and ACARS are sent in the clear, and public flight trackers are built on them. Federal law ([18 U.S.C. 2511](https://www.law.cornell.edu/uscode/text/18/2511)) treats unscrambled radio as readily accessible.
- ADS-B Out has been required in most controlled US airspace since January 2020 (FAA, [14 CFR 91.225](https://www.law.cornell.edu/cfr/text/14/91.225)), which is why nearly every plane shows.
- [FCC Part 87](https://www.law.cornell.edu/cfr/text/47/part-87) covers the voice band: only aircraft and airport stations may transmit.

## Try first

Wait a minute for the map, click a plane, then the play button on its guessed channel. [Tune 121.5](tune:121.5) is the guard channel, quiet unless something is wrong.
