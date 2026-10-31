# receiver

RTL-SDR receiver for Omarchy (Hyprland). Tauri 2 shell + Rust engine + Svelte 5 UI.

## Layout
- `crates/radio/` — engine, no UI deps. `device.rs` (librtlsdr FFI + `SimSource`), `dsp.rs`, `demod.rs` (WFM/NFM/AM/USB/LSB → 48 kHz), `spectrum.rs` (FFT rows), `detect.rs` (signals above floor), `bands.rs` + `default_bands.toml`, `theme.rs` (Omarchy colors), `audio.rs` (cpal), `engine.rs` (receive thread, `Command`/`Row`/`Status`).
- `src-tauri/` — commands `setup bands theme start tune set_mode set_gain set_volume set_playing`; polls theme + bands file every 2 s and emits `theme` / `bands` events.
- `crates/radio/src/sonde/` — weather-balloon decoders. `Scanner` gives each narrow detected signal a `Channel` (mix + 2-stage decimate to 48 kHz FM) and runs every `FrameDecoder` on it; `fsk.rs` symbol recovery; `dfm.rs` (DFM-06/09/17), `rs41.rs` (+ `rs.rs` Reed-Solomon, `geo.rs` ECEF). Channels that decode nothing for `TRY_FOR` are set aside for `RETRY_AFTER`. Engine runs it only after `Command::SetSondes(true)` (UI sends it in bands with `decoder = "radiosonde"`).
- `crates/radio/src/rds/` — FM station data. `phy.rs`: pilot PLL ×3 → 57 kHz mixdown, Costas for residual phase, PSK-rate timing (reuses `sonde::fsk::Fsk`), biphase pairing by energy, differential decode. `blocks.rs`: sync needs two blocks in order, 2-bit burst correction, loss at >42 bad of 50. `mod.rs`: PS (0A/0B), RadioText (2A/2B), RT+ title/artist. Runs inside the WFM `Demodulator` on the 240 kHz MPX.
- `BACKLOG.md` — ideas not yet started, one line each. Append when the user says "add to the backlog"; delete the line when it ships.
- `ui/` — Vite root. `lib/radio.ts` holds pure logic (labels, card placement, snap-to-tune) with tests in `radio.test.ts`. `lib/channels.ts`: fixed channel plans (CB, FRS, NOAA, marine, AAR rail) keyed by band id, so they apply even to an old user bands file; bookmarks still win over channel names.

## Wire format
Spectrum rows: Tauri `Channel<InvokeResponseBody::Raw>` — 8-byte f64 LE capture centre, then `FFT_SIZE` u8 bins. Byte 0 = `floor - BELOW_FLOOR_DB`, 255 = `RANGE_DB` above that (constants in `engine.rs`, sent to UI via `setup`).

## Commands
- Engine tests: `cargo test -p radio --release`
- Real-dongle probe: `cargo run -p radio --release --example probe -- 101.1 wfm 4`
- Stage timings: `cargo run -p radio --release --example bench`
- UI tests / types: `pnpm test`, `pnpm check`
- Decode a recording: `cargo run -p radio --release --example sonde_file -- <wav>` (mono = FM audio, stereo = I/Q). Verified 2026-10-30 on rs1729's `rs41/wav/20140717_402MHz.wav` (RS41 K1930308, 114 frames) and `iq/dfmIQ.wav` (DFM-06 314377).
- RDS offline: `mpx_dump -- <MHz> <secs> <out.f32>` records MPX, `rds_file -- <out.f32>` prints pilot lock, block stats and decoded info.
- Live sweep of 400–406 MHz: `cargo run -p radio --release --example sonde_scan -- <secs per step>`.
- HD Radio sidebands across 88–108 MHz: `cargo run -p radio --release --example hd_scan -- <secs per step> [MHz… to profile]`.
- App: `./run.sh` (`pnpm tauri dev`; installs deps if missing, refuses while gqrx/SDR++ runs). `./run.sh sim` sets `RECEIVER_SIM=1` for the simulated receiver.

## Facts
- Dongle on eagle: generic RTL2832U + R820T, 24–1766 MHz, no HF. DVB driver blacklisted in `/etc/modprobe.d/blacklist-rtlsdr.conf`.
- Capture is 2.4 Msps (`SAMPLE_RATE`); UI view span capped at 0.8× that. Wider views would need sweeping — not built.
- User bands file: `~/.config/receiver/bands.toml`, written from defaults on first start; never overwritten.
- Theme source: `~/.local/state/omarchy/current/theme/colors.toml`.
- Sonde types near eagle (NWS list 09-25): every nearby site flies GRAW DFM-17; Denver (KDNR) lists none. RS41 is the other US type. Simulator includes a DFM-17 at 403.3 MHz.
- `cargo build` while `pnpm tauri dev` is rebuilding corrupts the shell crate's link (undefined hidden symbol): stop dev, `cargo clean -p receiver`, rebuild.
- The dongle streams via `rtlsdr_read_async` on its own thread into a `QUEUE`-buffer channel. Blocking `read_sync` dropped 15–25% of samples (found 2026-10-30: rows/s ~24 not 30, pilot phase random, RDS dead).
- eagle's dongle crystal is ~105 ppm slow (pilot measured at 18997.4 Hz).
- Only one program can hold the dongle; close gqrx/SDR++ first.
