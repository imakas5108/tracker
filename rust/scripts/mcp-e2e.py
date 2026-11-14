#!/usr/bin/env python3
"""End-to-end MCP acceptance test: a scripted agent analyses a video through tracker-mcp.

Drives the real stdio bridge (which launches tracker-headless), runs import -> inspect ->
mark -> calibrate -> track -> review -> measurements -> dataset -> Sheets rows -> xlsx/csv
export -> save -> reopen, and checks the results.

Usage: python3 rust/scripts/mcp-e2e.py [VIDEO]   (default: ~/Tracker/videos/BallTossOut.mp4)
TRACKER_APP=path runs against another app (for example target/release/tracker, the window);
KEEP_APP=1 leaves it running afterwards.
Build first: cargo build --release -p tracker-mcp -p tracker-app
Writes only to a temporary folder. Exits 0 on success.
"""
import base64, json, os, subprocess, sys, tempfile, time, uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target" / "release"
VIDEO = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / "Tracker/videos/BallTossOut.mp4"
JAVA_MARKS = {12: (176.100, 154.661), 14: (221.648, 118.624), 16: (267.195, 100.605), 18: (312.742, 99.103), 20: (358.290, 116.121), 22: (404.838, 150.156), 24: (450.886, 202.711), 26: (497.935, 272.784), 28: (545.485, 361.877)}

if not VIDEO.exists():
    print(f"SKIPPED: {VIDEO} not found"); sys.exit(0)

tmp = Path(tempfile.mkdtemp(prefix="tracker-e2e-", dir="/tmp"))
sock = tmp / "ctl" / "control.sock"
env = dict(os.environ, TRACKER_DATASETS_DIR=str(tmp / "datasets"))
APP = Path(os.environ.get("TRACKER_APP", BIN / "tracker-headless"))
bridge = subprocess.Popen([str(BIN / "tracker-mcp"), "--app", str(APP), "--socket", str(sock)],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(tmp / "bridge.log", "w"), text=True, bufsize=1, env=env)
serial = 0
checks = []

def rpc(method, params):
    global serial
    serial += 1
    bridge.stdin.write(json.dumps({"jsonrpc": "2.0", "id": serial, "method": method, "params": params}) + "\n"); bridge.stdin.flush()
    while True:
        line = bridge.stdout.readline()
        if not line:
            raise SystemExit("bridge closed stdout; see " + str(tmp / "bridge.log"))
        msg = json.loads(line)
        if msg.get("id") == serial:
            return msg

def call(_tool, **args):
    tool = TOOLS[_tool]
    if not tool.get("annotations", {}).get("readOnlyHint") and "request_id" not in args:
        args["request_id"] = str(uuid.uuid4())
    res = rpc("tools/call", {"name": _tool, "arguments": args})["result"]
    return res

def ok(_tool, **args):
    res = call(_tool, **args)
    if res.get("isError"):
        raise SystemExit(f"{_tool} failed: {res['content'][0]['text']}")
    return res["structuredContent"]

def check(label, cond, detail=""):
    checks.append((label, bool(cond)))
    print(("PASS " if cond else "FAIL ") + label + (f"  ({detail})" if detail else ""))

def rev(sid):
    return ok("tracker_get_session", session_id=sid)["revision"]

t0 = time.time()
init = rpc("initialize", {"protocolVersion": "2025-11-25", "clientInfo": {"name": "e2e-agent", "version": "1"}, "capabilities": {}})
check("initialize", "result" in init, f"{time.time()-t0:.1f}s incl. app launch")
bridge.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n"); bridge.stdin.flush()
tools = rpc("tools/list", {})["result"]["tools"]
TOOLS = {t["name"]: t for t in tools}
check("tools/list", len(tools) >= 36, f"{len(tools)} tools")
check("mutations not marked idempotent", not TOOLS["tracker_start_tracking"]["annotations"]["idempotentHint"])

imp = ok("tracker_import_video", path=str(VIDEO))
sid = imp["session_id"]
job = ok("tracker_get_job", job_id=imp["job_id"], wait_seconds=20)
check("import completes", job["state"] == "completed")
s = ok("tracker_get_session", session_id=sid)
check("frame index", s["frame_count"] > 0, f"{s['frame_count']} frames, first at {s['first_frame_time']:.4f}s")

res = call("tracker_get_frame", session_id=sid, time=10/30, crop={"x": 100, "y": 170, "width": 64, "height": 64}, max_edge=256)
img = [c for c in res["content"] if c["type"] == "image"]
check("frame image returned as MCP image content", img and base64.b64decode(img[0]["data"])[1:4] == b"PNG")
(tmp / "crop.png").write_bytes(base64.b64decode(img[0]["data"]))

track = ok("tracker_add_track", session_id=sid, expected_revision=rev(sid), name="ball")["track_id"]
ok("tracker_mark_point", session_id=sid, expected_revision=rev(sid), track_id=track, time=10/30, x=131.277, y=207.858)
ok("tracker_set_calibration", session_id=sid, expected_revision=rev(sid), x1=272.2005, y1=446.1109, x2=273.2010, y2=47.4032, length=1.0, unit="m")
ok("tracker_set_axes", session_id=sid, expected_revision=rev(sid), origin_x=131.277, origin_y=207.858, angle_deg=2.5221)
stale = call("tracker_add_track", session_id=sid, expected_revision=0)
check("stale revision refused", stale.get("isError") and "stale_revision" in stale["content"][0]["text"])

job = ok("tracker_start_tracking", session_id=sid, expected_revision=rev(sid), track_id=track, from_time=10/30)
reviews = 0
while True:
    job = ok("tracker_get_job", job_id=job["job_id"], wait_seconds=30)
    if job["state"] == "completed":
        break
    if job["state"] != "awaiting_review":
        raise SystemExit(f"unexpected job state: {job}")
    reviews += 1
    r = ok("tracker_get_review", session_id=sid)
    cand = r["candidate"]
    # A careful agent would inspect the crop; this script skips uncertain frames.
    call("tracker_get_frame", session_id=sid, time=r["time"], crop={"x": max(0, int(cand["x"]) - 40), "y": max(0, int(cand["y"]) - 40), "width": 80, "height": 80} if cand else None)
    job = ok("tracker_review_match", session_id=sid, job_id=job["job_id"], action="skip")["job"]
check("tracking finished", True, f"{reviews} uncertain frames skipped")

m = ok("tracker_get_measurements", session_id=sid, track_id=track, limit=1000)
errs = []
for r in m["measurements"]:
    f = round(r["time"] * 30)
    if f in JAVA_MARKS:
        jx, jy = JAVA_MARKS[f]
        errs.append(((r["pixel_x"] - jx) ** 2 + (r["pixel_y"] - jy) ** 2) ** 0.5)
check("agrees with Java Tracker hand marks", len(errs) >= 8 and max(errs) < 3, f"{len(errs)} frames, max {max(errs):.2f}px")
g = (m["fit"]["ax"] ** 2 + m["fit"]["ay"] ** 2) ** 0.5
check("fitted free-fall acceleration", 9.0 < g < 10.6, f"{g:.2f} m/s^2 from {m['total']} points")

ok("tracker_dataset_create", name="Ball toss e2e", description="MCP acceptance test")
added = ok("tracker_dataset_add_run", dataset="Ball toss e2e", session_id=sid, track_id=track, conditions={"trial": "1", "ball_mass_kg": "0.2"})
check("run stored in dataset", added["runs"] == 1, f"{added['observations']} observations")
rows = ok("tracker_dataset_rows", dataset="Ball toss e2e", sheet="observations", limit=1000)
check("Sheets values rows", rows["values"][0][:3] == ["run_id", "t_s", "frame_value"] and len(rows["values"]) == rows["total_rows"] + 1)
ok("tracker_dataset_export", dataset="Ball toss e2e", format="xlsx", path=str(tmp / "balltoss.xlsx"))
ok("tracker_dataset_export", dataset="Ball toss e2e", format="csv", sheet="runs", path=str(tmp / "runs.csv"))
check("xlsx and csv written", (tmp / "balltoss.xlsx").stat().st_size > 2000 and (tmp / "runs.csv").read_text().startswith("run_id,"))
dup = call("tracker_dataset_export", dataset="Ball toss e2e", format="csv", sheet="runs", path=str(tmp / "runs.csv"))
check("no silent overwrite", dup.get("isError") and "destination_exists" in dup["content"][0]["text"])

ok("tracker_save_project", session_id=sid, path=str(tmp / "balltoss.tracker.json"))
ok("tracker_close_session", session_id=sid)
re = ok("tracker_open_project", path=str(tmp / "balltoss.tracker.json"))
ok("tracker_get_job", job_id=re["job_id"], wait_seconds=20)
m2 = ok("tracker_get_measurements", session_id=re["session_id"], track_id=track, limit=1000)
check("project round trip", m2["total"] == m["total"] and m2["unit"] == "m")
act = ok("tracker_get_activity", limit=100)["activity"]
check("actions attributed to the agent", any(a["actor"] == "e2e-agent" for a in act))

bridge.stdin.close(); bridge.wait(timeout=10)
if not os.environ.get("KEEP_APP"):
    subprocess.run(["pkill", "-f", f"{APP.name} --agent-control --socket {sock}"])
failed = [c for c, passed in checks if not passed]
print(f"\n{len(checks) - len(failed)}/{len(checks)} checks passed in {time.time()-t0:.1f}s. Artifacts: {tmp}")
sys.exit(1 if failed else 0)
