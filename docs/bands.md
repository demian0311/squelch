# The bands file

Squelch reads its band list from `~/.config/squelch/bands.toml`. The first start writes it from the [defaults](../crates/radio/src/default_bands.toml), and Squelch never overwrites it after that. Saved edits show up within a few seconds, with no restart.

This page is the full format. It is written so that a person or an AI agent can add a band or a list of local repeaters without reading the code.

## Shape

The file is [TOML](https://toml.io). It has two kinds of table, both repeated: `[[group]]` and `[[band]]`.

```toml
[[group]]
id = "local"
name = "Local repeaters"

[[band]]
id = "local-2m"
group = "local"
name = "2 m repeaters"
icon = "tower"
mode = "nfm"
low_mhz = 145.1
high_mhz = 147.4
start_mhz = 146.94
names = [
  { mhz = 146.940, name = "W0ABC Denver" },
  { mhz = 147.225, name = "K0XYZ Boulder" },
]
```

## Groups

A group is a heading in the band rail. Groups show in file order.

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Short name that bands point to. |
| `name` | yes | Heading text in the rail. |


## Bands

Bands show under their group, in file order. The first nine in the rail get the keys 1–9.

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Short unique name. Squelch remembers the last frequency per `id`. Some ids have a special meaning; see below. |
| `group` | yes | The `id` of a `[[group]]`. A band whose group does not exist is left out of the rail, with no error. |
| `name` | yes | Label in the rail. |
| `icon` | yes | One of `radio plane cloud anchor walkie train wave tower pin msg balloon key mesh dish star therm`. An unknown name draws `radio`. |
| `mode` | yes | How to demodulate inside the band: `wfm` (broadcast FM), `nfm` (narrow FM, most two-way radio), `am`, `usb`, `lsb`, `none` (no audio, look only), or `p25` (digital trunked police and fire: tune a control channel and its unencrypted Phase 1 calls play; needs mbelib). |
| `low_mhz` | yes | Bottom edge, in MHz. |
| `high_mhz` | yes | Top edge, in MHz. |
| `start_mhz` | yes | Where to tune the first time the band opens. Keep it between `low_mhz` and `high_mhz`. |
| `decoder` | no | Only `radiosonde` (the weather-balloon decoder) works. Any other value shows a "decoder is not built yet" note. Leave it out for voice bands. |
| `names` | no | What frequencies in this band are for; see below. A band with none takes the shipped ones for its `id`. |
| `bookmarks` | no | Frequencies you saved; see below. |

Rules:

- The dongle's tuner covers 24–1766 MHz. Below 24 MHz it samples the antenna straight (direct sampling), which reads true up to 14.4 MHz, so AM radio, shortwave and the 80, 40 and 20 m ham bands work with no tuner gain; 14.4–24 MHz is out of reach. A band outside those ranges opens but shows nothing.
- One screen shows at most about 1.9 MHz. A wider band works, but you scroll to see all of it.
- Bands can overlap. At a given frequency, the narrowest band wins: its `mode` is used and its name is shown. A small band of local repeaters inside a wider ham band therefore takes over for its range.

## Names and bookmarks

A name says what a frequency is for. It goes in the `names` list of any band, and it needs no action from you in the app. A bookmark is a frequency you saved. The app writes `bookmarks` when you click the bookmark icon beside the name in the top bar, and hand edits stay as written.

| Field | Required | Meaning |
|---|---|---|
| `mhz` | yes | Frequency in MHz. For a repeater, use its **output** (the frequency it transmits on, the one you listen to). |
| `name` | yes | Shown on the signal's card. |
| `mode` | no | Overrides the band's `mode` for this frequency. |

A bookmark can also carry `notes = "free text"`.

A signal takes a name when it is within half its own width, or within 5 kHz, of that frequency. A bookmark replaces a name on the same frequency. Names and bookmarks from every band apply everywhere, not only inside their own band.

There is no field for a repeater's input offset or its CTCSS tone (the sub-audible tone that opens the repeater). A receiver does not need either one to listen. Put them in the `name` if you want to see them, for example `"W0ABC +0.6 100.0"`.

## Special band ids

These ids do more than label a band. Keep them on the band they belong to.

| `id` | Effect |
|---|---|
| `fm` | The band opened at start. Without it, the first band opens. |
| `iss` | Shows space station pass times and a dot in the rail while it may be overhead. |
| `cb`, `frs`, `wx`, `marine`, `rail` | Name signals from a built-in channel plan (CB, FRS/GMRS, NOAA weather, marine VHF, AAR railroad). A bookmark at the same frequency wins over the channel name. |
| `rail`, `mil`, and every band in group `data` | Hidden from the rail until the "Dead air" box is ticked. |

## When the file is wrong

If the file does not parse, Squelch uses the defaults and shows the error, with a line number, in the footer of the window. Fix the file and save it; the error clears on the next reload.

A missing required field is an error. An unknown field is ignored.

## Making a repeater list with an agent

Give the agent this page and say where you are. A prompt that works:

> Read docs/bands.md in the squelch repo. Add a group "Local repeaters" to ~/.config/squelch/bands.toml with one band per ham band (2 m, 70 cm) covering the repeaters within 50 miles of ZIP 80301. Use each repeater's output frequency and put its callsign, city and tone in the name. Do not change existing bands.

Check the result by opening Squelch (`./run.sh`, or `./run.sh sim` with no dongle) and opening the new band. If the file has an error, it shows in the footer.
