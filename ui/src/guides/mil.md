# Military aircraft

Military planes talk in AM between 225 and 400 MHz: tankers, fighters, transports, training ranges. Long silences, then a burst of callsigns and numbers.

## What you will hear

- Tanker refuelling tracks, fighter formations checking in, transports calling their base, and military towers and approach controllers.
- Callsigns are words and numbers ("REACH 341", "VIPER 21"). Much of it is brief and clipped.
- A lot is quiet. Near an air base or a training area (a MOA) a weekday morning is the busy time. Encrypted and frequency-hopping radios sound like noise or clicks.
- Nobody publishes these frequencies; hobbyists share what they hear.

## How this radio reads it

AM, like civil aviation, so two at once both come through. The band is 175 MHz wide and the dongle hears 2.4 MHz at once, so the waterfall shows a slice; drag it or type a frequency to look elsewhere. The detector marks what comes on, and any signal above the floor opens the squelch.

![A wide band, one slice at a time](diagram/capture)

## Band plan

| MHz | What |
|---|---|
| 243.0 | Military emergency guard (twice 121.5) |
| 225–400 | Air-to-air, air-to-ground, military towers and approach |
| 25 kHz | Channel spacing |

## Rules

- This is federal spectrum, managed by [NTIA](https://www.ntia.gov/) (the government's own spectrum office), not the FCC.
- Listening needs nothing; the law on readily accessible radio ([18 U.S.C. 2511](https://www.law.cornell.edu/uscode/text/18/2511)) covers it.
- Nothing may be transmitted here by anyone outside the military.

## Try first

[Tune 243.0](tune:243.0), then drag the waterfall toward 300 MHz and watch for a carrier that comes and goes.
