"""Isolated, paired release comparisons. Owns only apps and synthetic sessions it creates."""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('client', ROOT/'scripts/test-mcp.py')
client_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client_module)


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered)-1, round((len(ordered)-1)*fraction))]


def trial(name, app, clip, count, out):
    runtime = tempfile.mkdtemp(prefix='tbench-', dir='/private/tmp')
    env = dict(os.environ, TRACKER_CONTROL_DIRECTORY=runtime)
    log = (out/f'{name}-{count}-{time.time_ns()}.log').open('w')
    process = subprocess.Popen([str(app/'Contents/MacOS/Tracker'), '--agent-control', '--benchmark-background'],
                               env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
    c = None
    try:
        endpoint = Path(runtime)/'control.sock'
        deadline = time.monotonic()+20
        while not endpoint.exists():
            assert process.poll() is None, f'{name} exited during startup'
            if time.monotonic()>deadline: raise TimeoutError(f'{name} startup')
            time.sleep(.02)
        c = client_module.Client(env=env)
        sessions = []
        imports = []
        for i in range(count):
            path = out/f'video-{i}.mp4'
            if not path.exists(): os.link(clip, path)
            assert path.samefile(clip), f'{path} is not the requested fixture; use a fresh output directory'
            began = time.monotonic()
            sid = c.wait(c.mutate('import_video', path=str(path)))['session_id']
            imports.append(time.monotonic()-began)
            tid = c.mutate('add_track', sid, name='Controlled synthetic target')['track_id']
            c.wait(c.mutate('seek', sid, time=0))
            c.mutate('mark_point', sid, track_id=tid, time=0, x=130, y=850)
            c.mutate('configure_tracking', sid, template_radius=12, search_radius=64,
                     minimum_score=.9, minimum_separation=.015)
            sessions.append((sid, tid))
        latencies, states, peak = [], {}, 0
        began = time.monotonic()
        jobs = [c.mutate('start_tracking', sid, track_id=tid) for sid, tid in sessions]
        while len(states)<count:
            if time.monotonic()-began>120: raise TimeoutError('tracking')
            for job in jobs:
                if job['job_id'] in states: continue
                before = time.monotonic(); status = c.tool('get_job', job_id=job['job_id'])
                latencies.append((time.monotonic()-before)*1000)
                if status['state'] in ['completed','failed','awaiting_review','stopped']:
                    assert status['state']=='completed', status
                    states[job['job_id']] = status
            peak = max(peak, int(subprocess.check_output(['ps','-p',str(process.pid),'-o','rss='],text=True))*1024)
            time.sleep(.01)
        elapsed = time.monotonic()-began
        outputs = []
        for sid, tid in sessions:
            rows = c.tool('get_measurements', session_id=sid, track_id=tid)['measurements']
            assert len(rows)==72
            outputs.append([{k:r[k] for k in ['time','pixel_x','pixel_y','origin','frame']} for r in rows])
        result = dict(variant=name, videos=count, wall_s=elapsed, automatic_fps=71*count/elapsed,
                      import_s=imports, reply_p95_ms=percentile(latencies,.95), reply_max_ms=max(latencies),
                      rss_peak_bytes=peak, outputs=outputs, pid=process.pid)
        print(json.dumps({k:v for k,v in result.items() if k!='outputs'}), flush=True)
        return result
    finally:
        if c: c.close()
        # This process was created above, with a private endpoint and test-only input paths.
        process.terminate()
        try: process.wait(timeout=8)
        except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=3)
        log.close()
        shutil.rmtree(runtime)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--variant',action='append',required=True,help='name=/absolute/Tracker.app')
    parser.add_argument('--counts',default='1,2,4,8')
    parser.add_argument('--repetitions',type=int,default=5)
    parser.add_argument('--output',default='build/controlled/results')
    args = parser.parse_args()
    out = (ROOT/args.output).resolve(); out.mkdir(parents=True,exist_ok=True)
    client_module.OUT.mkdir(parents=True,exist_ok=True)
    variants = [(s.split('=',1)[0],Path(s.split('=',1)[1]).resolve()) for s in args.variant]
    clip = ROOT/'build/audit/balltoss/synthetic-ball-toss-clean.mp4'
    report = dict(fixture_sha256=hashlib.sha256(clip.read_bytes()).hexdigest(),
                  swift=subprocess.check_output(['swift','--version'],text=True),
                  macos=subprocess.check_output(['sw_vers'],text=True),trials=[])
    for count in map(int,args.counts.split(',')):
        for repetition in range(args.repetitions):
            order = variants if repetition%2==0 else list(reversed(variants))
            for name, app in order:
                entry = trial(name,app,clip,count,out); entry['repetition'] = repetition
                report['trials'].append(entry)
                (out/'raw.json').write_text(json.dumps(report,indent=2))
    summary = []
    for count in map(int,args.counts.split(',')):
        for name,_ in variants:
            group = [r for r in report['trials'] if r['variant']==name and r['videos']==count]
            fps = [r['automatic_fps'] for r in group]
            summary.append(dict(variant=name,videos=count,repetitions=len(group),fps_median=statistics.median(fps),
                fps_min=min(fps),fps_max=max(fps),reply_p95_ms=statistics.median([r['reply_p95_ms'] for r in group]),
                rss_peak_bytes=max(r['rss_peak_bytes'] for r in group)))
    reference = report['trials'][0]['outputs'][0]
    for trial_result in report['trials']:
        for rows in trial_result['outputs']:
            for a,b in zip(reference,rows):
                assert a['frame']==b['frame'] and a['origin']==b['origin']
                assert all(abs(a[k]-b[k])<1e-6 for k in ['time','pixel_x','pixel_y'])
    (out/'summary.json').write_text(json.dumps(dict(scientific_equivalence=True,results=summary),indent=2))


if __name__=='__main__': main()
