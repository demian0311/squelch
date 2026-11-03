#!/usr/bin/env python3
"""Writes crates/radio/src/fm_list.txt: licensed US FM stations from the FCC's FM Query.

One line each: call MHz lat lon kW HAAT-metres state city. HAAT is antenna height above average terrain.
Translators and low-power stations are other services and are left out. The list has no format or HD field.
Run: python3 -I ui/scripts/fmlist.py
"""
import pathlib
import urllib.request

URL = "https://transition.fcc.gov/fcc-bin/fmq?state=&serv=FM&status=3&list=4&size=9"
OUT = pathlib.Path(__file__).resolve().parent.parent.parent / "crates" / "radio" / "src" / "fm_list.txt"


def number(text):
    try:
        return float(text.split()[0])
    except (IndexError, ValueError):
        return 0.0


def main():
    with urllib.request.urlopen(URL, timeout=120) as r:
        text = r.read().decode("latin-1")
    best = {}
    for line in text.splitlines():
        f = [v.strip() for v in line.split("|")]
        if len(f) < 27 or f[9] != "LIC":
            continue
        call = f[1].removesuffix("-FM")
        lat = (number(f[20]) + number(f[21]) / 60 + number(f[22]) / 3600) * (1 if f[19] == "N" else -1)
        lon = (number(f[24]) + number(f[25]) / 60 + number(f[26]) / 3600) * (-1 if f[23] == "W" else 1)
        kw = max(number(f[14]), number(f[15]))
        haat = max(number(f[16]), number(f[17]))
        row = (kw, number(f[2]), lat, lon, haat, f[11], f[10].title())
        # a station with several records, keep the strongest
        if call not in best or row[0] > best[call][0]:
            best[call] = row
    rows = sorted(best.items(), key=lambda c: (c[1][1], c[0]))
    OUT.write_text("".join(f"{c} {m:.1f} {la:.2f} {lo:.2f} {k:g} {h:.0f} {s} {t}\n" for c, (k, m, la, lo, h, s, t) in rows))
    print(f"{len(rows)} stations, {OUT.stat().st_size} bytes")


main()
