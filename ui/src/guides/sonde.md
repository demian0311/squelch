# Weather balloons

Twice a day, 92 weather service sites let go a balloon with a radiosonde under it. For two hours it reports where it is and what the air is like, in the clear, near 403 MHz.

## What you will hear

- Nothing to listen to: the signal is data. The panel lists each sonde heard with its position, height, climb and the weather it reads.
- Launches go up just before 00:00 and 12:00 UTC (early evening and early morning in the US). The sonde climbs at about 5 m/s for 90 minutes or more, bursts above 30 km, and comes down on a small parachute 50 to 150 miles downwind.
- With a view of the horizon you hear one from 150 miles away once it is high.

![A flight, launch to landing](diagram/sonde-flight)

## How this radio reads it

The sonde sends a frame a second: GPS position, temperature, humidity, pressure. The scanner gives every narrow signal in 400–406 MHz its own channel and tries each decoder on it. Two types fly in the US: GRAW DFM-17 at most sites, Vaisala RS41 at the rest; both are read here. A hobbyist map of live sondes is at [SondeHub](https://sondehub.org/).

## Band plan

| MHz | What |
|---|---|
| 400–406 | Meteorological aids: radiosondes |
| 403 | Where most US sondes sit |
| 1680 | The other sonde band, not covered by this dongle |

## Rules

- A federal government service ([National Weather Service upper-air programme](https://www.weather.gov/upperair/)); nothing for the listener to do. Listening needs nothing.
- A landed sonde is discarded by the weather service. Finders keep them; hobbyists chase them with this data.

## Try first

[Tune 403.0](tune:403.0) within an hour after 00:00 or 12:00 UTC and watch the panel.
