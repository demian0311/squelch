#!/usr/bin/env python3
"""Writes crates/radio/src/am_list.txt: licensed AM stations from the FCC's AM Query.

One line each: call kHz lat lon day-kW night-kW class state city. Night kW is 0 for a daytime-only station.
Class is the FCC's station class (47 CFR 73.21): A clear channel, B regional, C local, D daytime or low at night.
A station's records (day, night, unlimited) fold into one line; the coordinates are its strongest record's.
Run: python3 -I ui/scripts/amlist.py
"""
import pathlib
import urllib.request

URL = "https://transition.fcc.gov/fcc-bin/amq?state=&serv=AM&status=3&list=4&size=9"
OUT = pathlib.Path(__file__).resolve().parent.parent.parent / "crates" / "radio" / "src" / "am_list.txt"


def number(text):
    try:
        return float(text.split()[0])
    except (IndexError, ValueError):
        return 0.0


def main():
    req = urllib.request.Request(URL, headers={"User-Agent": "curl/8.0"})
    with urllib.request.urlopen(req, timeout=180) as r:
        text = r.read().decode("latin-1")
    best = {}
    for line in text.splitlines():
        f = [v.strip() for v in line.split("|")]
        if len(f) < 27 or f[9] != "LIC" or f[3] != "AM":
            continue
        call, khz, hours, cls = f[1], int(number(f[2])), f[5], f[7] or "-"
        kw = number(f[14])
        lat = (number(f[20]) + number(f[21]) / 60 + number(f[22]) / 3600) * (1 if f[19] == "N" else -1)
        lon = (number(f[24]) + number(f[25]) / 60 + number(f[26]) / 3600) * (-1 if f[23] == "W" else 1)
        key = (call, khz)
        s = best.setdefault(key, {"day": 0.0, "night": 0.0, "kw": -1.0, "cls": cls})
        if hours in ("DAY", "UNL"):
            s["day"] = max(s["day"], kw)
        if hours in ("NIG", "UNL"):
            s["night"] = max(s["night"], kw)
        if kw > s["kw"]:
            s.update(kw=kw, lat=lat, lon=lon, state=f[11], city=f[10].title(), cls=cls)
    rows = sorted(best.items(), key=lambda c: (c[0][1], c[0][0]))
    OUT.write_text(
        "".join(
            f"{c} {k} {s['lat']:.2f} {s['lon']:.2f} {s['day']:g} {s['night']:g} {s['cls']} {s['state'] or '-'} {s['city'] or '-'}\n"
            for (c, k), s in rows
        )
    )
    print(f"{len(rows)} stations, {OUT.stat().st_size} bytes")


main()
