"""Generate reproducible, synthetic tracking clips; requires ffmpeg on PATH."""
import json
from pathlib import Path
import random
import subprocess
import tempfile

DEST = Path(__file__).resolve().parents[1] / "build" / "fixtures"
DEST.mkdir(parents=True, exist_ok=True)
for scenario in ("clean", "noise", "occlusion", "distractor"):
    expected = []
    elapsed = 0.0
    with tempfile.TemporaryDirectory(prefix="tracker-fixture-") as folder:
        folder = Path(folder)
        entries = []
        for frame in range(60):
            rng = random.Random(frame)
            x, y = 30 + frame, 45 + frame // 4
            pixels = [40] * (160 * 120)
            if scenario == "noise":
                pixels = [max(0, min(255, value + rng.randint(-15, 15))) for value in pixels]
            def draw(cx, cy):
                for dy in range(-6, 7):
                    for dx in range(-6, 7):
                        value = 230 if (dx + dy) % 3 == 0 else 120
                        if scenario == "noise":
                            value += rng.randint(-10, 10)
                        pixels[(cy + dy) * 160 + cx + dx] = value
            if not (scenario == "occlusion" and 20 <= frame < 30):
                draw(x, y)
            if scenario == "distractor" and frame >= 20:
                draw(x + 18, y)
            rgb = bytes(channel for pixel in pixels for channel in (pixel, pixel, pixel))
            path = folder / f"{frame:03d}.ppm"
            path.write_bytes(b"P6\n160 120\n255\n" + rgb)
            duration = 0.04 if frame % 2 == 0 else 0.08
            entries.extend([f"file '{path}'", f"duration {duration}"])
            expected.append({"time": round(elapsed, 6), "x": x, "y": y})
            elapsed += duration
        manifest = folder / "frames.txt"
        manifest.write_text("\n".join(entries) + "\n")
        subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0",
                        "-i", str(manifest), "-fps_mode", "vfr", "-c:v", "libx264", "-crf", "0", "-bf", "0",
                        "-pix_fmt", "yuv420p", str(DEST / f"{scenario}.mov")], check=True)
        (DEST / f"{scenario}.json").write_text(json.dumps(expected, indent=2))
print(f"Synthetic VFR fixtures and annotations: {DEST}")
