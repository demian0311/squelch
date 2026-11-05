# Space station

The International Space Station carries a ham radio. On 145.800 MHz it talks to schools, sends pictures, and relays other hams, for about ten minutes each time it passes over.

## What you will hear

- Most passes: nothing, or the cross-band repeater: hams on the ground calling each other through the station, heard on 437.800 MHz.
- A school contact: an astronaut answering students' questions on 145.800 MHz, FM, a few times a month somewhere in the world.
- SSTV events: a tone that sounds like a fax for two minutes, which is a picture. A decoder on a phone, held to the speaker, shows it.
- Packet data near 437.825 MHz: short chirps, position reports relayed through the station.

![A pass: up over one horizon, down the other, ten minutes at most](diagram/iss-pass)

## How this radio reads it

Narrow FM at 145.800 MHz. The station moves at 17,000 mph, so its signal arrives about 3.5 kHz high as it rises and 3.5 kHz low as it sets; the FM channel is wide enough that you just hear it. The Space station panel works out the passes offline from the orbit data (a TLE from [CelesTrak](https://celestrak.org/), refreshed every few hours) and your location, says how high each one climbs (higher is louder and longer), and draws the ground track; the rail's dot says it may be overhead now.

## Band plan

| MHz | What |
|---|---|
| 145.800 | Voice down, SSTV down |
| 145.990 | Cross-band repeater uplink (hams on the ground, 67 Hz tone) |
| 437.800 | Cross-band repeater downlink |
| 437.825 | Packet data |
| 145.825 | Where packet used to be |

## Rules

- [FCC Part 97](https://www.law.cornell.edu/cfr/text/47/part-97) on the ham side; the station's own licence is Russian and American. Listening needs nothing.
- Talking through the repeater needs a Technician licence and a few watts; a handheld with a small beam antenna does it.

## Try first

Pick the next pass in the Space station panel, [tune 145.8](tune:145.8) at its start, and hold the window open to the sky.
