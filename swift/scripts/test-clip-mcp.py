"""Clip acceptance through the packaged bridge. Use an isolated app endpoint."""
import csv
import importlib.util
import json
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'build/audit/clips'
spec = importlib.util.spec_from_file_location('motion_acceptance', ROOT/'scripts/test-motion-mcp.py')
motion = importlib.util.module_from_spec(spec)
spec.loader.exec_module(motion)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    motion.client_module.OUT.mkdir(parents=True, exist_ok=True)
    c = motion.client_module.Client()
    checks = []
    try:
        assert len(c.rpc('tools/list', {})['result']['tools']) == 25
        project = motion.fixture_project()
        original = project['analysis']['tracks'][0]['points']
        path = OUT/'annotated-v1.tracker.json'
        path.write_text(json.dumps(project))
        sid = c.wait(c.mutate('open_project', path=str(path)))['session_id']
        tid = project['analysis']['selectedTrackID']
        assert c.tool('get_session', session_id=sid)['clip']['full_source']
        checks.append('legacy full-source default')
        args = dict(session_id=sid, expected_revision=c.tool('get_session', session_id=sid)['revision'],
                    request_id=str(uuid.uuid4()), start_frame=3, end_frame=12, step_size=2)
        op = c.tool('set_clip', **args)
        assert c.tool('set_clip', **args) == op
        state = c.wait(op)['result']
        selection = state['clip']
        assert (selection['sample_count'], selection['last_sample_frame']) == (5,11)
        motion.near(state['time'], original[3]['time'])
        conflict = c.raw('set_clip', **dict(args, end_frame=13))
        assert conflict['structuredContent']['code'] == 'request_id_conflict'
        stale = c.raw('set_clip', **dict(args, request_id=str(uuid.uuid4())))
        assert stale['structuredContent']['code'] == 'stale_revision'
        revision = state['revision']
        bad = c.raw('set_clip', session_id=sid, expected_revision=revision, request_id=str(uuid.uuid4()),
                    start_frame=5, end_frame=3, step_size=1)
        assert bad['structuredContent']['code'] == 'invalid_clip'
        assert c.tool('get_session', session_id=sid)['revision'] == revision
        c.wait(c.mutate('set_clip', sid, start_frame=3, end_frame=12, step_size=2))
        assert c.tool('get_session', session_id=sid)['revision'] == revision
        checks.append('retry deduplication, stale rejection, invalid/no-op edits preserve revision')
        data = c.tool('get_measurements', session_id=sid, track_id=tid, limit=2)
        assert data['total'] == 5 and data['retained_total'] == 72 and data['next_offset'] == 2
        rows = c.tool('get_measurements', session_id=sid, track_id=tid)['measurements']
        for row, index in zip(rows, [3,5,7,9,11]):
            assert row['frame'] == original[index]['frame']
            motion.near(row['time'], original[index]['time'])
        for i in range(1,4):
            h = rows[i+1]['time']-rows[i]['time']
            motion.near(rows[i]['vx'], (rows[i+1]['x']-rows[i-1]['x'])/(2*h))
        assert rows[0]['vx'] is None and rows[-1]['vx'] is None
        checks.append('selected pagination, retained counts, source stamps and sampled derivatives')
        before = c.tool('get_session', session_id=sid)
        image = c.raw('get_frame', session_id=sid, time=original[30]['time'], max_edge=320)
        assert any(block['type'] == 'image' for block in image['content'])
        assert image['structuredContent']['frame'] == original[30]['frame']
        assert c.tool('get_session', session_id=sid)['time'] == before['time']
        c.wait(c.mutate('seek', sid, time=original[10]['time']))
        assert c.tool('get_session', session_id=sid)['frame'] == original[9]['frame']
        c.wait(c.mutate('seek', sid, time=original[70]['time']))
        assert c.tool('get_session', session_id=sid)['frame'] == original[11]['frame']
        checks.append('independent full-source image inspection and selected-step seek clamping')
        saved = OUT/'selected-v4.tracker.json'
        c.wait(c.mutate('save_project', sid, path=str(saved), overwrite=True))
        document = json.loads(saved.read_text())
        assert document['version'] == 4 and len(document['analysis']['tracks'][0]['points']) == 72
        standard = OUT/'selected-standard.csv'
        c.wait(c.mutate('export_csv', sid, track_id=tid, path=str(standard), overwrite=True))
        table = list(csv.DictReader(standard.open()))
        assert len(table) == 5 and len(table[0]) == 22
        assert all((r['clip_start_frame'],r['clip_end_frame'],r['clip_step_size']) == ('3','12','2') for r in table)
        excel = OUT/'selected-excel.csv'
        c.wait(c.mutate('export_csv', sid, track_id=tid, path=str(excel), overwrite=True,
                        profile='excel', decimal_separator='comma'))
        lines = excel.read_text(encoding='utf-8-sig').splitlines()
        assert lines[0] == 'sep=;' and len(list(csv.reader(lines[1:], delimiter=';'))[0]) == 8
        checks.append('version-4 save retains all 72 points; standard and regional Excel export select 5')
        c.mutate('undo', sid)
        # Viewer refresh is asynchronous for clip undo, as for the native undo action.
        until = time.monotonic()+10
        while c.tool('get_session', session_id=sid)['seeking']:
            assert time.monotonic()<until
            time.sleep(.02)
        assert c.tool('get_measurements', session_id=sid, track_id=tid)['total'] == 72
        c.mutate('redo', sid)
        until = time.monotonic()+10
        while c.tool('get_session', session_id=sid)['seeking']:
            assert time.monotonic()<until
            time.sleep(.02)
        reopened = c.wait(c.mutate('open_project', path=str(saved)))['session_id']
        assert c.tool('get_session', session_id=reopened)['clip'] == selection
        assert c.tool('get_measurements', session_id=reopened, track_id=tid)['total'] == 5
        c.wait(c.mutate('reset_clip', reopened))
        assert c.tool('get_measurements', session_id=reopened, track_id=tid)['total'] == 72
        old_page = c.raw('get_measurements', session_id=sid, track_id=tid, expected_revision=revision, offset=2)
        assert old_page['structuredContent']['code'] == 'stale_revision'
        checks.append('undo/redo, reopen/reset restores retained points, stale pagination rejected')
        # A separate import exercises the actual matcher rather than annotated points.
        clean = ROOT/'build/fixtures/clean.mov'
        tracked = c.wait(c.mutate('import_video', path=str(clean)))['session_id']
        target = c.mutate('add_track', tracked, name='Clip seed')['track_id']
        c.wait(c.mutate('set_clip', tracked, start_frame=3, end_frame=12, step_size=2))
        t = c.tool('get_session', session_id=tracked)['time']
        c.mutate('mark_point', tracked, track_id=target, time=t, x=33, y=45)
        c.mutate('configure_tracking', tracked, template_radius=8, search_radius=24, minimum_score=.9, minimum_separation=.015)
        finished = c.wait(c.mutate('start_tracking', tracked, track_id=target))
        assert finished['processed_frames'] == 4
        assert finished['clip']['last_sample_frame'] == 11
        observed = c.tool('get_measurements', session_id=tracked, track_id=target)
        assert observed['total'] == 5 and observed['measurements'][-1]['time'] == finished['clip']['last_sample_time']
        c.wait(c.mutate('reset_clip', tracked))
        retained = c.tool('get_job', job_id=finished['job_id'])
        assert retained['clip'] == finished['clip']
        checks.append('bounded autotracking inspects four selected frames; job retains immutable clip after reset')
        refresh = c.mutate('set_clip', tracked, start_frame=15, end_frame=21, step_size=2)
        c.mutate('cancel_job', job_id=refresh['job_id'])
        until = time.monotonic()+10
        while c.tool('get_job', job_id=refresh['job_id'])['state'] in ('queued','running'):
            assert time.monotonic()<until
            time.sleep(.02)
        assert c.tool('get_session', session_id=tracked)['clip']['start_frame'] == 15
        c.close()
        c = motion.client_module.Client()
        assert c.tool('get_session', session_id=tracked)['clip']['start_frame'] == 15
        assert c.tool('get_job', job_id=refresh['job_id'])['state'] in ('completed','cancelled')
        c.wait(c.mutate('seek', tracked, time=0))
        assert c.tool('get_session', session_id=tracked)['frame'] == c.tool('get_session', session_id=tracked)['clip']['start_stamp']
        checks.append('cancelled viewer refresh preserves accepted clip and status across reconnect; explicit seek recovers viewer')
        mismatch = json.loads(saved.read_text())
        mismatch['analysis']['clip']['startStamp']['value'] += 1
        wrong = OUT/'wrong-boundary-v4.tracker.json'
        wrong.write_text(json.dumps(mismatch))
        failed = c.wait(c.mutate('open_project', path=str(wrong)), states=('failed',))
        assert failed['error']['code'] == 'media_error' and 'clip does not match' in failed['error']['message']
        checks.append('source boundary mismatch fails open without a modal relink chooser')
        report = dict(checks=checks, clip=selection, job=finished, selected_rows=rows)
        (OUT/'acceptance.json').write_text(json.dumps(report,indent=2))
        print(json.dumps(dict(passed=len(checks),checks=checks),indent=2))
    finally:
        c.close()


if __name__ == '__main__':
    main()
