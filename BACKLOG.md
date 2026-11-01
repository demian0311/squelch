# Backlog

Ideas to work later. One line each, newest at the bottom; an indented note under a line if it needs context. Delete a line when it ships.

- Decode VDL Mode 2 (digital successor to ACARS, 136.x MHz) beside the ACARS panel.
- Replace the off-screen channel overlays at the waterfall's left and right edges with a better-looking cue.
  Keep the idea of showing what lies beyond each edge of the view, but the current overlays look poor; needs a design pass first.
- Show Wi-Fi access points by channel and nearby Bluetooth LE devices with signal strength, beside the dongle.
  Findings in `docs/radios.md`: both readable without root via NetworkManager / BlueZ D-Bus; no Wi-Fi monitor mode or per-channel noise on this card.
