#!/usr/bin/env python3
"""Read dump1090's Beast output (port 30005) and emit NDJSON, one GeoJSON point per position message.

The BaseStation feed (port 30003) carries decoded positions but drops the signal
level, so we read the Beast binary feed instead: every frame there pairs the raw
Mode S message with dump1090's measured signal level. Positions are decoded here
by pyModeS, which needs a couple of messages per aircraft before it commits to a
CPR fix, so decoded messages wait in a short hold buffer before being emitted.
"""

import argparse
import json
import math
import socket
import sys
import time
from collections import deque
from datetime import UTC, datetime

import pyModeS as pms

HOST = "127.0.0.1"
PORT = 30005
HOLD = 4.0  # seconds to hold a message while pyModeS decides on its position
STALE = 300.0  # seconds before an aircraft's remembered callsign/velocity expires
SURFACE_REF = None  # receiver (lat, lon), needed to decode surface positions

ESC = 0x1A
MSG_LEN = {0x31: 2, 0x32: 7, 0x33: 14}  # beast frame type -> Mode S message bytes
SIGNAL = 6  # signal level byte, right after the 6-byte MLAT timestamp
HEADER_LEN = 7  # timestamp + signal level, before the Mode S message itself
POSITION_BDS = ("0,5", "0,6")  # airborne, surface
CARRY = {  # fields remembered from an aircraft's other messages -> property name
    "callsign": "callsign",
    "squawk": "squawk",
    "groundspeed": "gs",
    "track": "track",
    "vertical_rate": "vr",
}


def unescape(buf, start, want):
    """Read `want` bytes from buf[start:], undoing beast's doubling of 0x1a.

    Returns (bytes, index just past them, whether all `want` bytes were there).
    """
    out = bytearray()
    i, n = start, len(buf)
    while len(out) < want:
        if i >= n or (buf[i] == ESC and i + 1 >= n):
            return out, n, False  # need more of the stream
        if buf[i] != ESC:
            out.append(buf[i])
            i += 1
        elif buf[i + 1] == ESC:
            out.append(ESC)
            i += 2
        else:
            return out, i, False  # truncated frame; a new one starts at i
    return out, i, True


def take_frames(buf):
    """Pop complete beast frames off buf as (frame type, unescaped body) pairs.

    Bytes belonging to a frame that hasn't fully arrived yet are left in buf.
    """
    frames = []
    pos = 0
    while True:
        start = buf.find(ESC, pos)
        if start < 0:  # nothing here can start a frame
            pos = len(buf)
            break
        if start + 1 >= len(buf):
            pos = start  # wait for the frame type byte
            break
        kind = buf[start + 1]
        if (msg_len := MSG_LEN.get(kind)) is None:
            pos = start + (2 if kind == ESC else 1)  # unknown frame type; resync
            continue
        body, end, complete = unescape(buf, start + 2, HEADER_LEN + msg_len)
        if not complete:
            if end >= len(buf):
                pos = start  # wait for the rest of the frame
                break
            pos = end  # truncated frame; pick up at the next one
            continue
        frames.append((kind, bytes(body)))
        pos = end
    del buf[:pos]
    return frames


def rssi_dbfs(signal):
    """Convert a beast signal byte (dump1090 sends sqrt(power) * 255) to dBFS."""
    return round(20 * math.log10(signal / 255), 1) if signal else None


def feature(recv_time, signal, decoded, carried):
    lat, lon = decoded.get("latitude"), decoded.get("longitude")
    if lat is None or lon is None:
        return None
    return {
        "type": "Feature",
        "geometry": {"type": "Point", "coordinates": [round(lon, 6), round(lat, 6)]},
        "properties": {
            "time": datetime.fromtimestamp(recv_time, UTC).isoformat(
                timespec="milliseconds"
            ),
            "hex": decoded["icao"],
            "rssi": rssi_dbfs(signal),
            "altitude": decoded.get("altitude"),
            "on_ground": decoded.get("bds") == "0,6",
            "typecode": decoded.get("typecode"),
            **carried,
        },
    }


def drain(held, cutoff):
    """Emit held messages older than `cutoff`, dropping any still without a position."""
    while held and held[0][0] <= cutoff:
        if line := feature(*held.popleft()):
            print(json.dumps(line), flush=True)


def format_remaining(seconds):
    minutes, secs = divmod(int(seconds + 0.5), 60)
    return f"{minutes}m{secs:02d}s"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "-s",
        "--sample-seconds",
        type=float,
        default=60.0,
        help="seconds to read before exiting (default: %(default)s)",
    )
    args = parser.parse_args()

    decoder = pms.PipeDecoder(surface_ref=SURFACE_REF)
    aircraft = {}  # icao -> last seen carryable fields, plus "seen" timestamp
    held = deque()  # (recv_time, signal, decoded dict, carried fields snapshot)
    buf = bytearray()
    start = time.time()
    deadline = start + args.sample_seconds
    swept = start
    last_progress = 0.0

    try:
        with socket.create_connection((HOST, PORT), timeout=1.0) as sock:
            while True:
                try:
                    chunk = sock.recv(65536)
                except TimeoutError:
                    chunk = b""  # idle feed; fall through to drain the hold buffer
                else:
                    if not chunk:
                        break  # dump1090 closed the connection
                    buf += chunk
                now = time.time()
                if args.sample_seconds > 0 and now - last_progress >= 0.25:
                    last_progress = now
                    remaining = max(deadline - now, 0.0)
                    if remaining > 0:
                        frac = 1.0 - remaining / args.sample_seconds
                        print(
                            f"\r{frac * 100:5.1f}% done, {format_remaining(remaining)} left  ",
                            end="",
                            file=sys.stderr,
                            flush=True,
                        )

                for kind, body in take_frames(buf):
                    if kind not in (0x32, 0x33):  # Mode A/C frames carry no position
                        continue
                    try:
                        decoded = decoder.decode(
                            body[HEADER_LEN:].hex().upper(), timestamp=now
                        )
                    except pms.DecodeError:
                        continue
                    if not (icao := decoded.get("icao")):
                        continue
                    seen = aircraft.setdefault(icao, {})
                    seen["seen"] = now
                    for field, prop in CARRY.items():
                        value = decoded.get(field)
                        if value is not None:
                            seen[prop] = (
                                round(value, 1) if isinstance(value, float) else value
                            )
                    if decoded.get("bds") in POSITION_BDS and decoded.get("crc_valid"):
                        # pyModeS backfills lat/lon on this dict once it has enough
                        # frames to trust the fix, so hold onto it rather than a copy
                        carried = {k: v for k, v in seen.items() if k != "seen"}
                        held.append((now, body[SIGNAL], decoded, carried))

                drain(held, now - HOLD)
                if now - swept > STALE:
                    aircraft = {
                        k: v for k, v in aircraft.items() if now - v["seen"] < STALE
                    }
                    swept = now
                if now >= deadline:
                    print(
                        "\r100.0% done, 0m00s left  ",
                        file=sys.stderr,
                        flush=True,
                    )
                    break
    except KeyboardInterrupt:
        pass

    decoder.flush()
    drain(held, time.time())
    if args.sample_seconds > 0:
        print(file=sys.stderr, flush=True)
