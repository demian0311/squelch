# Shortwave

International broadcasters, time signals and utility stations between 2.3 and 14 MHz. The signals bounce off the sky, so what you hear changes with the hour and the season.

## What you will hear

- Broadcasters in the metre bands below: news in many languages, religious stations, and the big state broadcasters that are left. Each sits on a 5 kHz step.
- WWV, the US time signal from Fort Collins, Colorado, on 2.5, 5 and 10 MHz: a tick every second, a voice announcing the minute, and the time in both Morse-like tones and speech.
- Between the broadcast bands: aircraft crossing oceans, ships, the military and amateurs, most of it single sideband or data.
- Nothing by day on the low bands and nothing by night on the high ones: the 49 and 41 m bands are evening and night bands, 31 m works both, 25 and 22 m are day and dusk bands.

## How this radio reads it

Below 24 MHz the dongle samples the antenna straight (direct sampling), with no tuner gain or filter in front: the antenna sets the level, and a whip hears little. Direct sampling reads true only up to 14.4 MHz, half the dongle's clock, so the 19 m band and everything above it is out of reach; the band here stops at 13.9 MHz. AM is the mode for broadcasters; switch to USB or LSB for voices that sound like ducks.

## Band plan

| Band | MHz | When |
|---|---|---|
| 120 m | 2.3–2.495 | Night, tropical broadcasters |
| 90 m | 3.2–3.4 | Night |
| 75 m | 3.9–4.0 | Night |
| 60 m | 4.75–4.995 | Night |
| 49 m | 5.9–6.2 | Evening and night, the busiest |
| 41 m | 7.2–7.45 | Evening and night |
| 31 m | 9.4–9.9 | Night; days in winter |
| 25 m | 11.6–12.1 | Day and dusk |
| 22 m | 13.57–13.87 | Day |

The ranges are the [WRC-97 allocations](https://en.wikipedia.org/wiki/Shortwave_bands); [WWV](https://www.nist.gov/pml/time-and-frequency-division/time-distribution/radio-station-wwv) is NIST's.

## Rules

- International broadcasting from the US is licensed under [FCC Part 73, subpart F](https://www.law.cornell.edu/cfr/text/47/part-73/subpart-F). Listening needs nothing, anywhere in the band.
- The space between the broadcast bands belongs to aviation, maritime, amateur and government services; listening is legal, repeating what you hear from a private service is not.

## Try first

[Tune 10.000](tune:10.0/am) MHz for WWV at any hour; then [9.6 MHz](tune:9.6/am) in the evening and step 5 kHz at a time through the 31 m band.
