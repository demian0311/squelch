#!/usr/bin/env python3
"""Writes ui/src/lib/maplines.json: US state lines and highways for the local map.

States: us-atlas 3.0.1 states-10m (Census cartographic boundaries, ISC).
Highways: Natural Earth 10m roads (public domain), major and secondary, inside US boxes.
Coordinates are rounded to 0.01 degrees (about 1 km), which is all a 400 km map shows.
Run: python3 -I ui/scripts/maplines.py
"""
import json
import pathlib
import urllib.request

STATES = "https://cdn.jsdelivr.net/npm/us-atlas@3.0.1/states-10m.json"
ROADS = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_10m_roads.geojson"
OUT = pathlib.Path(__file__).resolve().parent.parent / "src" / "lib" / "maplines.json"
# lower 48, Alaska, Hawaii: (west, south, east, north)
BOXES = [(-125.5, 24, -66, 49.5), (-170, 51, -129, 72), (-161, 18.5, -154, 22.5)]
KINDS = {"Major Highway": 1, "Beltway": 1, "Secondary Highway": 0}


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "squelch maplines"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


def inside(x, y):
    return any(w <= x <= e and s <= y <= n for w, s, e, n in BOXES)


def squeeze(line):
    out = []
    for x, y in line:
        p = [round(x, 2), round(y, 2)]
        if not out or out[-1] != p:
            out.append(p)
    return out if len(out) > 1 else None


def states():
    t = fetch(STATES)
    (sx, sy), (tx, ty) = t["transform"]["scale"], t["transform"]["translate"]
    out = []
    for arc in t["arcs"]:
        x = y = 0
        pts = []
        for dx, dy in arc:
            x += dx
            y += dy
            pts.append((x * sx + tx, y * sy + ty))
        line = squeeze(pts)
        if line:
            out.append(line)
    return out


def roads():
    out = []
    for f in fetch(ROADS)["features"]:
        kind = KINDS.get(f["properties"].get("type"))
        g = f["geometry"]
        if kind is None or not g:
            continue
        for ln in g["coordinates"] if g["type"] == "MultiLineString" else [g["coordinates"]]:
            if any(inside(x, y) for x, y in ln):
                line = squeeze(ln)
                if line:
                    out.append({"major": kind, "c": line})
    return out


OUT.write_text(json.dumps({"states": states(), "roads": roads()}, separators=(",", ":")))
print(OUT, OUT.stat().st_size)
