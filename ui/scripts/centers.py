#!/usr/bin/env python3
"""Writes crates/radio/src/centers.txt: the radio sites of the en-route centres (ARTCC) and the frequencies each one
keeps, from the FAA's NASR 28-day subscription (public domain, https://www.faa.gov/air_traffic/flight_info/aeronav/aero_data/NASR_Subscription/).

Above 18,000 ft a plane talks to a centre, not an airport, through the remote site (RCAG) nearest its sector. Which
sector is not in the data, so the app lists every VHF frequency of the nearest site and lights the busy ones.

One line per site, tab separated: centre id, centre name, site name, lat, lon, then channels as use:MHz;... where use
is LOW, HIGH, LOW/HIGH or ULTRA-HIGH (the FAA's SECTORIZATION). VHF only: the air band is 118-137 MHz.
Run: python3 -I ui/scripts/centers.py <cycle date, e.g. 2026-10-29>   (downloads FRQ_CSV and ARB_CSV for that cycle)
  or python3 -I ui/scripts/centers.py <FRQ.csv> <ARB_BASE.csv>         (files already unzipped)
"""
import csv
import datetime
import io
import pathlib
import sys
import urllib.request
import zipfile

OUT = pathlib.Path(__file__).resolve().parents[2] / "crates" / "radio" / "src" / "centers.txt"
BASE = "https://nfdc.faa.gov/webContent/28DaySub/extra/"


def fetch_csv(cycle: str, name: str, inner: str) -> str:
    day = datetime.date.fromisoformat(cycle)
    url = f"{BASE}{day.strftime('%d_%b_%Y')}_{name}_CSV.zip"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req) as r:
        z = zipfile.ZipFile(io.BytesIO(r.read()))
    return z.read(inner).decode("utf8")


if len(sys.argv) == 2:
    frq_text = fetch_csv(sys.argv[1], "FRQ", "FRQ.csv")
    arb_text = fetch_csv(sys.argv[1], "ARB", "ARB_BASE.csv")
elif len(sys.argv) == 3:
    frq_text = pathlib.Path(sys.argv[1]).read_text(encoding="utf8")
    arb_text = pathlib.Path(sys.argv[2]).read_text(encoding="utf8")
else:
    sys.exit(__doc__)

names = {r["LOCATION_ID"]: r["LOCATION_NAME"].title() for r in csv.DictReader(io.StringIO(arb_text)) if r["LOCATION_TYPE"] == "ARTCC"}
sites: dict[tuple[str, str], dict] = {}
for r in csv.DictReader(io.StringIO(frq_text)):
    centre = r["ARTCC_OR_FSS_ID"]
    if r["FACILITY_TYPE"] != "RCAG" or centre not in names:
        continue
    try:
        mhz = float(r["FREQ"])
    except ValueError:
        continue
    # VHF only, and not the guard channel every site keeps: the air band already names 121.5
    if not 118 <= mhz < 137 or mhz == 121.5:
        continue
    s = sites.setdefault((centre, r["FACILITY"]), {"lat": float(r["LAT_DECIMAL"]), "lon": float(r["LONG_DECIMAL"]), "ch": {}})
    s["ch"].setdefault(mhz, r["SECTORIZATION"] or "LOW/HIGH")

lines = []
for (centre, site), s in sorted(sites.items()):
    chans = ";".join(f"{use}:{mhz:g}" for mhz, use in sorted(s["ch"].items()))
    lines.append("\t".join([centre, names[centre], site.title(), f"{s['lat']:.4f}", f"{s['lon']:.4f}", chans]))
OUT.write_text("\n".join(lines) + "\n")
print(len(lines), "sites,", sum(len(s["ch"]) for s in sites.values()), "frequencies,", OUT.stat().st_size, "bytes")
