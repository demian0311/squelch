# Squelch

A receiver for RTL-SDR dongles, built for [Omarchy](https://omarchy.org) on Hyprland. Plug in a dongle, pick a band, and click a signal on the waterfall to hear it.

## What it does

- **Spectrum and waterfall.** Live view of up to about 1.9 MHz around the tuned frequency.
- **Listen.** Wide FM, narrow FM, AM, USB and LSB.
- **Finds signals for you.** Signals that stand above the noise floor get a card on the waterfall. Click a card to tune to it.
- **Bands and channels.** Ready-made bands for broadcast FM, aircraft, NOAA weather, marine, CB, walkie-talkies, railroad, the ham bands and more. CB, FRS, NOAA, marine and US rail channels show their names.
- **FM station data (RDS).** Station name, RadioText, and song title and artist where the station sends them.
- **HD Radio.** Plays the digital HD channels of FM stations, through [nrsc5](https://github.com/theori-io/nrsc5). This is optional: Squelch still starts without it.
- **Weather balloons.** Decodes the radiosondes that weather services launch twice a day: Graw DFM-06, DFM-09 and DFM-17, and Vaisala RS41.
- **Follows your Omarchy theme.** It recolors when you change the theme.

## Hardware

Any RTL2832U dongle should work. Squelch was built on one with an R820T tuner, which covers 24–1766 MHz. HF (below 24 MHz) is not supported.

Only one program can use the dongle at a time, so close gqrx or SDR++ first.

## Build and run

On Arch:

```sh
sudo pacman -S rtl-sdr webkit2gtk-4.1 rust pnpm
git clone https://github.com/demian0311/squelch.git && cd squelch
./run.sh
```

`./run.sh sim` starts a simulated receiver, so you can try Squelch with no dongle.

If the kernel's DVB-T TV driver claims the dongle first, blacklist it:

```sh
echo 'blacklist dvb_usb_rtl28xxu' | sudo tee /etc/modprobe.d/blacklist-rtlsdr.conf
```

Then unplug the dongle and plug it back in.

For HD Radio, install `nrsc5-git` from the AUR, or build nrsc5 and put `libnrsc5.so` in `~/.local/lib`.

## Configuration

The band list is in `~/.config/squelch/bands.toml`. It is written from the defaults on first start, and Squelch never overwrites it. Edits show up within a few seconds, with no restart.

[docs/bands.md](docs/bands.md) describes every field, so you or an AI agent can add your own bands and local repeaters.

## Development

```sh
cargo test -p radio --release   # engine tests
pnpm test                       # UI tests
pnpm check                      # type check
```

`crates/radio/examples/` has command-line tools that work without the UI. They probe a station, scan for weather balloons or HD Radio, and decode recordings.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
