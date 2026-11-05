# EMS

Police, fire and ambulance dispatch. Most cities run a digital trunked system on 700 or 800 MHz; many counties still dispatch in plain FM on VHF or UHF. One rail entry covers both.

## What you will hear

- Dispatch: a calm voice sending units to an address, the unit answering, "en route", "on scene", "clear".
- Fire ground, EMS to hospital, police tactical: each is a talkgroup, which is who is talking rather than a frequency.
- Encrypted talkgroups are silent here (counted, not played). Many police departments encrypt everything; fire and EMS mostly do not.

## How this radio reads it

A trunked system shares a few channels among hundreds of talkgroups. One channel, the control channel, is always on and says which talkgroup is on which channel right now. This receiver finds the control channel, reads it, and jumps to each call. It follows P25 Phase 1, the digital voice most US public safety systems use.

![How a trunked system hands out channels](diagram/trunking)

On the VHF and UHF bands the receiver just plays whatever opens the squelch: plain narrow FM, one channel per agency. The EMS screen's look sweeps the whole band every minute and lists the carriers that come and go like voice.

## Band plan

| MHz | What |
|---|---|
| 769–775 | 700 MHz public safety, towers talk here |
| 851–869 | 800 MHz public safety, towers talk here |
| 150–160 | VHF: fire, EMS and rural police in plain FM |
| 450–470 | UHF: the same, in cities |
| 155.7525 | VCALL10: calling, any agency, the same everywhere |
| 154.280 | VFIRE21: fire mutual aid |
| 155.475 | VLAW31: police mutual aid |
| 851.0125 | 8CALL90: calling on 800 MHz (plain FM) |

## Rules

- [FCC Part 90](https://www.law.cornell.edu/cfr/text/47/part-90), licensed to the agencies. Listening needs nothing: federal law ([18 U.S.C. 2511](https://www.law.cornell.edu/uscode/text/18/2511)) names police and fire radio as readily accessible to the public.
- Some states restrict a scanner in a car, or using one while committing a crime. Check yours before mounting one.
- Encrypted calls are not yours to break; this receiver does not try.

## Try first

[Try the 800 MHz towers](band:ps800): the screen shows the search, the lock and the calls. Or [the VHF band](band:psvhf) in a rural county.
