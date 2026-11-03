#!/usr/bin/env python3
"""Writes ui/src/lib/airlines.json from OpenFlights airlines.dat (Open Database License, https://openflights.org/data.php).

ICAO code to IATA code and name, active airlines only. Run: python3 -I ui/scripts/airlines.py
"""
import csv
import io
import json
import pathlib
import urllib.request

URL = "https://raw.githubusercontent.com/jpatokal/openflights/master/data/airlines.dat"
OUT = pathlib.Path(__file__).resolve().parent.parent / "src" / "lib" / "airlines.json"
TRIM = (" Airlines", " Air Lines", " Airways", " International", " Aviation", " Airline", " Inc", " Air Service", " Air Services")

with urllib.request.urlopen(URL, timeout=60) as r:
    rows = csv.reader(io.StringIO(r.read().decode("utf8")))
    out = {}
    for row in rows:
        name, iata, icao, active = row[1], row[3], row[4], row[7]
        if active != "Y" or len(icao) != 3 or not icao.isalpha() or not name:
            continue
        for t in TRIM:
            name = name.removesuffix(t)
        iata = iata if len(iata) == 2 else ""
        # a code with two rows: keep the one that has an IATA code
        if icao not in out or (iata and not out[icao][0]):
            out[icao] = [iata, name]
OUT.write_text(json.dumps(dict(sorted(out.items())), ensure_ascii=False, separators=(",", ":")))
print(len(out), "airlines,", OUT.stat().st_size, "bytes")
