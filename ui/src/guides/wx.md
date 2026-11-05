# NOAA weather

The [National Weather Service](https://www.weather.gov/nwr/) reads the forecast around the clock on seven channels near 162.5 MHz. The one you hear is a transmitter a few miles away.

## What you will hear

- A synthesised voice reading the local forecast, current conditions, and any watches and warnings, on a loop of a few minutes.
- When a warning comes in the loop breaks for a burst of data, a 1050 Hz tone, and the warning. Weather radios wake up on that tone.
- More than 1,000 transmitters cover all 50 states; a city hears one or two of the seven channels, the nearest loudest.

![What happens when a warning comes in](diagram/wx-alert)

## How this radio reads it

Narrow FM, always on, so the squelch opens and stays open. The data burst before the tone is SAME (Specific Area Message Encoding), county codes for the warning; this receiver plays the voice and does not read the codes.

## Band plan

| MHz | What |
|---|---|
| 162.400 | WX1 |
| 162.425 | WX2 |
| 162.450 | WX3 |
| 162.475 | WX4 |
| 162.500 | WX5 |
| 162.525 | WX6 |
| 162.550 | WX7 |

## Rules

- Government stations; nothing in FCC rules for a listener.
- Nobody else may transmit on these channels.

## Try first

[Tune 162.475](tune:162.475), then try the others: the loudest is your transmitter.
