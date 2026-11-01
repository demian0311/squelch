# Backlog

Ideas to work later. One line each, newest at the bottom; an indented note under a line if it needs context. Delete a line when it ships.

- Decode ADS-B (aircraft position broadcasts) on the `adsb` band at 1090 MHz: callsign, altitude, speed, position, drawn on a map panel.
  The band exists with `decoder = "adsb"`, but nothing reads that decoder yet. The pulses are 1 Mbit/s, so the 2.4 Msps capture is just enough.
- Decode ACARS (short text messages between aircraft and airlines) on the `acars` band, 129–131.7 MHz: flight, registration, message text.
  The band exists with `decoder = "acars"`, but no decoder. US ACARS channels span 129.125–131.55 MHz, a little over one 2.4 MHz capture, so one tuning gets most of them; VDL Mode 2 (digital successor, 136.x MHz) could follow.
- Replace the off-screen channel overlays at the waterfall's left and right edges with a better-looking cue.
  Keep the idea of showing what lies beyond each edge of the view, but the current overlays look poor; needs a design pass first.
- Show Wi-Fi access points by channel and nearby Bluetooth LE devices with signal strength, beside the dongle.
  Findings in `docs/radios.md`: both readable without root via NetworkManager / BlueZ D-Bus; no Wi-Fi monitor mode or per-channel noise on this card.
