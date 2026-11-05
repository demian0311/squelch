# Railroad

Train crews, dispatchers and the trackside machines that talk to them, on 160–161 MHz. Every railroad in North America uses the same numbered channels.

## What you will hear

- Road channel: the crew and the dispatcher. Track warrants and signal indications read back word for word, "over" at the end.
- Yard channels: switching, "three cars to a joint", a lot of short calls.
- Defect detectors: a box by the track that scans a passing train for hot bearings and dragging gear, then says so in a synthesised voice: milepost, "no defects", axle count, "detector out".
- The end-of-train device: a short data chirp from the box on the last car, with the brake pressure, every minute or so.

![Who talks on a railroad](diagram/train)

## How this radio reads it

Narrow FM (12.5 kHz since the 2013 narrowbanding). The band's channels are numbered by the AAR (Association of American Railroads): channel 7 is 160.215 MHz and they climb 15 kHz at a time to 97 at 161.565; the Channels panel lists them with a dot for the ones on air. The end-of-train chirps are data on 457.9375 MHz, outside this band.

## Band plan

| MHz | What |
|---|---|
| 160.215–161.565 | AAR channels 7–97, 15 kHz apart |
| 160.800 | AAR 46, a common road channel |
| 457.9375 | End-of-train box (data) |
| 452.9375 | Head-of-train side (data) |

## Rules

- [FCC Part 90](https://www.law.cornell.edu/cfr/text/47/part-90) (private land mobile), licensed to the railroads. Listening needs nothing.
- Which channel a line uses is set by each railroad; [RadioReference](https://www.radioreference.com/) lists them by subdivision.
- Nobody else may transmit here.

## Try first

[Tune 160.8](tune:160.8), then look at the Channels panel for the one with a dot near a line you know.
