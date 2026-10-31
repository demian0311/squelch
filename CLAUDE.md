# receiver

RTL-SDR receiver for Omarchy (Hyprland). Tauri 2 shell + Rust engine + Svelte 5 UI.

## Layout
- `crates/radio/` — engine, no UI deps. `device.rs` (librtlsdr FFI + `SimSource`), `dsp.rs`, `demod.rs` (WFM/NFM/AM/USB/LSB → 48 kHz), `spectrum.rs` (FFT rows), `detect.rs` (signals above floor), `bands.rs` + `default_bands.toml`, `theme.rs` (Omarchy colors), `audio.rs` (cpal), `engine.rs` (receive thread, `Command`/`Row`/`Status`).
- `src-tauri/` — commands `setup bands theme start tune set_mode set_gain set_volume set_playing`; polls theme + bands file every 2 s and emits `theme` / `bands` events.
- `ui/` — Vite root. `lib/radio.ts` holds pure logic (labels, card placement, snap-to-tune) with tests in `radio.test.ts`.

## Wire format
Spectrum rows: Tauri `Channel<InvokeResponseBody::Raw>` — 8-byte f64 LE capture centre, then `FFT_SIZE` u8 bins. Byte 0 = `floor - BELOW_FLOOR_DB`, 255 = `RANGE_DB` above that (constants in `engine.rs`, sent to UI via `setup`).

## Commands
- Engine tests: `cargo test -p radio --release`
- Real-dongle probe: `cargo run -p radio --release --example probe -- 101.1 wfm 4`
- Stage timings: `cargo run -p radio --release --example bench`
- UI tests / types: `pnpm test`, `pnpm check`
- App: `pnpm tauri dev`. `RECEIVER_SIM=1` forces the simulated receiver.

## Facts
- Dongle on eagle: generic RTL2832U + R820T, 24–1766 MHz, no HF. DVB driver blacklisted in `/etc/modprobe.d/blacklist-rtlsdr.conf`.
- Capture is 2.4 Msps (`SAMPLE_RATE`); UI view span capped at 0.8× that. Wider views would need sweeping — not built.
- User bands file: `~/.config/receiver/bands.toml`, written from defaults on first start; never overwritten.
- Theme source: `~/.local/state/omarchy/current/theme/colors.toml`.
- Only one program can hold the dongle; close gqrx/SDR++ first.
