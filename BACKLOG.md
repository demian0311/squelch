# Backlog

Ideas to work later. One line each, newest at the bottom; an indented note under a line if it needs context. Delete a line when it ships.

- HD Radio (NRSC-5, the digital sidebands ±~130–200 kHz around an FM carrier): per-card toggle; play the digital audio (HD1/HD2…), show station name, title/artist, album art, station logo, traffic/weather images.
  - Plan: mix the station out of the 2.4 Msps capture (as `sonde::Channel` does), resample to NRSC-5's ~1.488 Msps, feed libnrsc5 (open-source decoder, theori-io/nrsc5) through its sample-pipe API over FFI. Audio → existing cpal path; images/metadata → events to the card.
  - Pipeline: dongle → capture → [mix + resample] → libnrsc5 → audio | ID3 text | LOT files (album art, logos, maps) → UI.
  - Unknowns: whether any eagle-area station broadcasts HD (check spectrum for sidebands first); libnrsc5 is GPL-3 and its audio codec is patented — matters once the project is public; not installed here (AUR only, as of 2026-10-31). Verify API names against the nrsc5 source before writing code.
