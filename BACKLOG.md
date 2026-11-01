# Backlog

Ideas to work later. One line each, newest at the bottom; an indented note under a line if it needs context. Delete a line when it ships.

- Record received audio to compressed files (e.g. Opus/MP3), started and stopped from the app.
- Label a frequency from inside the app, with a name and free-text notes (e.g. "county ambulance dispatch").
  Bookmarks exist today only as `bookmarks` in `~/.config/squelch/bands.toml`, edited by hand; the app reads them for names but cannot add or edit one.
- Favourite frequencies as a quick-access list in the app, one click to tune.
- Schedule of times to tune in (nets, prepper 3-3-3 check-ins): frequency, mode, recurrence, with a reminder when one is about to start.
- On an FM station, start HD Radio decoding in the background; show the HD panel only once it has pictures or digital audio.
  Today HD starts only on a station where the user already picked a program (localStorage `hd`); the panel appears as soon as HD starts, before it has anything to show.
- Decode ADS-B (aircraft position broadcasts) on the `adsb` band at 1090 MHz: callsign, altitude, speed, position, drawn on a map panel.
  The band exists with `decoder = "adsb"`, but nothing reads that decoder yet. The pulses are 1 Mbit/s, so the 2.4 Msps capture is just enough.
- Decode ACARS (short text messages between aircraft and airlines) on the `acars` band, 129–131.7 MHz: flight, registration, message text.
  The band exists with `decoder = "acars"`, but no decoder. US ACARS channels span 129.125–131.55 MHz, a little over one 2.4 MHz capture, so one tuning gets most of them; VDL Mode 2 (digital successor, 136.x MHz) could follow.
- Replace the off-screen channel overlays at the waterfall's left and right edges with a better-looking cue.
  Keep the idea of showing what lies beyond each edge of the view, but the current overlays look poor; needs a design pass first.
