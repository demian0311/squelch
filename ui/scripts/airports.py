#!/usr/bin/env python3
"""Writes crates/radio/src/airports.txt from OurAirports (public domain, https://ourairports.com/data/).

Kept: every US large and medium airport, US small ones with a radio channel, and the world's large and medium.
One line each, tab separated: ident, IATA, kind (L M S), lat, lon, name, city, then channels as type:MHz:description;...
Only channels a pilot talks on. Weather, unicom-less radio aids and the like are left out.
Run: python3 -I ui/scripts/airports.py
"""
import csv
import io
import pathlib
import urllib.request

BASE = "https://davidmegginson.github.io/ourairports-data/"
OUT = pathlib.Path(__file__).resolve().parent.parent.parent / "crates" / "radio" / "src" / "airports.txt"
KINDS = {"large_airport": "L", "medium_airport": "M", "small_airport": "S"}
TALK = {"TWR", "GND", "APP", "DEP", "A/D", "CTAF", "UNIC", "CNTR"}


def table(name):
    with urllib.request.urlopen(BASE + name, timeout=60) as r:
        return list(csv.DictReader(io.StringIO(r.read().decode("utf8"))))


def clean(s):
    return " ".join(s.replace("\t", " ").replace(";", ",").split())


channels = {}
for f in table("airport-frequencies.csv"):
    if f["type"] in TALK and f["frequency_mhz"]:
        channels.setdefault(f["airport_ident"], []).append(f"{f['type']}:{float(f['frequency_mhz']):g}:{clean(f['description']).replace(':', ' ')}")
lines = []
for a in table("airports.csv"):
    kind = KINDS.get(a["type"])
    if not kind or (kind == "S" and (a["iso_country"] != "US" or a["ident"] not in channels)):
        continue
    lines.append("\t".join([a["ident"], a["iata_code"], kind, f"{float(a['latitude_deg']):.4f}", f"{float(a['longitude_deg']):.4f}", clean(a["name"]), clean(a["municipality"]), ";".join(channels.get(a["ident"], []))]))
lines.sort()
OUT.write_text("\n".join(lines) + "\n")
print(len(lines), "airports,", OUT.stat().st_size, "bytes")
