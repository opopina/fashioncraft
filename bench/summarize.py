"""Summarise a cloth-fit metrics.jsonl: speed, settling and quality.

Usage: python3 -I bench/summarize.py <output-dir> [label]

sewn  = first step whose largest seam gap is under 3.5 mm (about the contact radius);
quiet = first step from which the RMS vertex speed stays under 1 cm/s and the seams stay sewn.
Times are given simulated and real (the sum of the step times up to that step).
"""
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
label = sys.argv[2] if len(sys.argv) > 2 else out.name
rows = [json.loads(line) for line in (out / "metrics.jsonl").read_text().splitlines() if line.strip()]
run = json.loads((out / "run.json").read_text())

SEWN_M = 0.0035
QUIET_M_S = 0.01


def speed(r):
    return (r.get("motion") or {}).get("speed_rms_m_s", float("nan"))


wall = [0.0]
for r in rows:
    wall.append(wall[-1] + r["step_ms"] / 1000.0)
total = wall[-1]
sim = rows[-1]["time_s"] if rows else 0.0
det = sum(r.get("detection_ms", 0.0) for r in rows) / 1000.0
gpu = sum(r.get("gpu_compute_ms") or 0.0 for r in rows) / 1000.0

sewn = next((i for i, r in enumerate(rows) if r["max_seam_gap_m"] < SEWN_M), None)
quiet = None
for i in range(len(rows) - 1, -1, -1):
    if speed(rows[i]) < QUIET_M_S and rows[i]["max_seam_gap_m"] < SEWN_M:
        quiet = i
    else:
        break


def at(i):
    if i is None:
        return "never"
    return f"{rows[i]['time_s']:.2f} s sim / {wall[i + 1]:.2f} s real"


last = rows[-1]
print(
    f"{label}: {run['vertices']} vertices, {len(rows)} steps, {sim:.2f} s simulated in {total:.2f} s"
    f" ({sim / total if total else 0:.2f}x real time), mean step {1000 * total / len(rows):.1f} ms,"
    f" detection {100 * det / total if total else 0:.0f}%, GPU compute {100 * gpu / total if total else 0:.0f}%"
)
print(f"  sewn (<{SEWN_M * 1000:.1f} mm) at {at(sewn)}; quiet (<{QUIET_M_S * 100:.0f} cm/s) from {at(quiet)}")
print(
    f"  final: seam gap {1000 * last['max_seam_gap_m']:.2f} mm, stretch p95 {last.get('extension_p95_percent', 0):.1f}%"
    f" max {last.get('extension_max_percent', 0):.1f}%, speed rms {100 * speed(last):.2f} cm/s,"
    f" contacts {last.get('contacts')}, min distance {1000 * (last.get('minimum_distance_m') or 0):.2f} mm"
)
