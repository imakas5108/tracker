"""Bounded, pixel-only clip acceptance on the previously inspected DJI footage."""
import argparse
import base64
import importlib.util
import json
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT/'build/audit/clips-dji'
spec = importlib.util.spec_from_file_location('client', ROOT/'scripts/test-mcp.py')
client = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('video', type=Path)
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    client.OUT.mkdir(parents=True, exist_ok=True)
    c = client.Client()
    try:
        began = time.monotonic()
        imported = c.wait(c.mutate('import_video', path=str(args.video.resolve())), timeout=120)
        import_seconds = time.monotonic()-began
        sid = imported['session_id']
        c.wait(c.mutate('set_clip', sid, start_frame=2526, end_frame=2586, step_size=1))
        state = c.tool('get_session', session_id=sid)
        assert abs(state['clip']['start_time']-84.2842)<1e-6
        assert state['clip']['sample_count'] == 61 and state['calibration'] is None
        preview = c.raw('get_frame', session_id=sid, time=state['time'])
        image = next(block for block in preview['content'] if block['type']=='image')
        (OUT/'release-preview.png').write_bytes(base64.b64decode(image['data']))
        tid = c.mutate('add_track', sid, name='Bounded DJI trial — pixels')['track_id']
        # Agent-selected source location from the earlier visual stress test, not ground truth.
        c.mutate('mark_point', sid, track_id=tid, time=state['time'], x=903, y=318)
        c.mutate('configure_tracking', sid, template_radius=12, search_radius=64,
                 minimum_score=.9, minimum_separation=.015, matching_method='ssd')
        began = time.monotonic()
        job = c.wait(c.mutate('start_tracking', sid, track_id=tid), states=('completed','awaiting_review'),timeout=120)
        track_seconds = time.monotonic()-began
        data = c.tool('get_measurements', session_id=sid, track_id=tid)
        assert data['total'] > 0 and all(row['unit'] == 'px' for row in data['measurements'])
        assert all(state['clip']['start_time'] <= row['time'] <= state['clip']['last_sample_time'] for row in data['measurements'])
        review = None
        if job['state'] == 'awaiting_review':
            review = c.tool('get_review', session_id=sid)
            assert review['pending'] and not review['recorded']
            assert all(row['time'] < review['time'] for row in data['measurements'])
        project = OUT/'bounded-dji-v4.tracker.json'
        c.wait(c.mutate('save_project', sid, path=str(project),overwrite=True))
        c.wait(c.mutate('export_csv', sid, track_id=tid,path=str(OUT/'bounded-dji-pixels.csv'),overwrite=True))
        reopened = c.wait(c.mutate('open_project', path=str(project)),timeout=120)['session_id']
        restored = c.tool('get_session',session_id=reopened)
        assert restored['clip'] == state['clip'] and restored['calibration'] is None
        report = dict(source=str(args.video.resolve()),bytes=args.video.stat().st_size,
                      import_s=import_seconds,tracking_s=track_seconds,clip=state['clip'],job=job,
                      measurements=data,review=review,reopened_session_id=reopened,
                      limitation='Bounded range enforcement; no ground-truth accuracy or physical calibration claim.')
        (OUT/'acceptance.json').write_text(json.dumps(report,indent=2))
        print(json.dumps(dict(import_s=import_seconds,tracking_s=track_seconds,job_state=job['state'],
                              processed=job['processed_frames'],recorded=data['total'],clip=state['clip']),indent=2))
    finally:
        c.close()


if __name__ == '__main__':
    main()
