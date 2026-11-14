"""Motion acceptance through the packaged bridge; use TRACKER_CONTROL_DIRECTORY for an isolated app."""
import csv
import importlib.util
import json
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'build/audit/motion'
spec = importlib.util.spec_from_file_location('tracker_mcp_client', ROOT / 'scripts/test-mcp.py')
client_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client_module)


def fixture_project():
    clip = ROOT / 'build/audit/balltoss/synthetic-ball-toss-clean.mp4'
    annotation = json.loads(clip.with_suffix('.json').read_text())
    track = str(uuid.uuid4())
    points = [dict(id=str(uuid.uuid4()), time=f['time_s'], pixel=[f['pixel_x'], f['pixel_y']],
                   frame=f['timestamp'], origin='manual') for f in annotation['frames']]
    return dict(version=1, source=clip.as_uri(), sourceSize=[1920, 1080], time=0,
                settings=dict(templateRadius=12, searchRadius=24, minimumScore=.9, minimumSeparation=.015),
                algorithm='fixed-grayscale-ssd-v1',
                analysis=dict(tracks=[dict(id=track, name='Annotated synthetic ball toss', points=points)],
                              selectedTrackID=track))


def near(actual, expected, tolerance=1e-7):
    assert actual is not None and abs(actual-expected) < tolerance, (actual, expected)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    fixture = fixture_project()
    legacy = OUT / 'annotated-v1.tracker.json'
    legacy.write_text(json.dumps(fixture))
    c = client_module.Client()
    try:
        assert len(c.rpc('tools/list', {})['result']['tools']) == 23
        sid = c.wait(c.mutate('open_project', path=str(legacy)))['session_id']
        tid = fixture['analysis']['selectedTrackID']
        state = c.tool('get_session', session_id=sid)
        assert state['motion']['window'] == 3
        assert state['motion']['origin_y_px'] == 1080
        c.mutate('set_calibration', sid, x1=1600, y1=933, x2=1700, y2=933, length=1, unit='m')
        axes_args = dict(session_id=sid, expected_revision=c.tool('get_session', session_id=sid)['revision'],
                         request_id=str(uuid.uuid4()), origin_x=130, origin_y=850, angle_degrees=0)
        changed = c.tool('set_axes', **axes_args)
        assert c.tool('set_axes', **axes_args) == changed
        conflict = c.raw('set_axes', **dict(axes_args, origin_x=131))
        assert conflict['structuredContent']['code'] == 'request_id_conflict'
        stale = c.raw('configure_motion', session_id=sid, expected_revision=axes_args['expected_revision'],
                      request_id=str(uuid.uuid4()), window=5)
        assert stale['structuredContent']['code'] == 'stale_revision'
        data = c.tool('get_measurements', session_id=sid, track_id=tid)
        rows = data['measurements']
        assert len(rows) == 72 and rows[0]['vx'] is None and rows[-1]['ay'] is None
        near(rows[0]['x'], 0); near(rows[0]['y'], 0)
        h = 1001/24000
        for i in range(1, 71):
            near(rows[i]['vx'], (rows[i+1]['x']-rows[i-1]['x'])/(2*h))
            near(rows[i]['vy'], (rows[i+1]['y']-rows[i-1]['y'])/(2*h))
            near(rows[i]['ax'], (rows[i+1]['x']-2*rows[i]['x']+rows[i-1]['x'])/h**2)
            near(rows[i]['ay'], (rows[i+1]['y']-2*rows[i]['y']+rows[i-1]['y'])/h**2)
        c.mutate('set_axes', sid, origin_x=130, origin_y=850, angle_degrees=90)
        rotated = c.tool('get_measurements', session_id=sid, track_id=tid)['measurements']
        for i in range(1, 71):
            near(rotated[i]['vx'], rows[i]['vy']); near(rotated[i]['vy'], -rows[i]['vx'])
            near(rotated[i]['ax'], rows[i]['ay']); near(rotated[i]['ay'], -rows[i]['ax'])
        c.mutate('undo', sid)
        c.wait(c.mutate('seek', sid, time=rows[20]['time']))
        c.mutate('mark_point', sid, track_id=tid, time=rows[20]['time'],
                 x=rows[20]['pixel_x']+1, y=rows[20]['pixel_y'])
        corrected = c.tool('get_measurements', session_id=sid, track_id=tid)['measurements']
        near(corrected[19]['vx'], rows[19]['vx']+1/(200*h))
        near(corrected[20]['ax'], rows[20]['ax']-2/(100*h*h))
        c.mutate('undo', sid)
        c.mutate('configure_motion', sid, window=7)
        smooth = c.tool('get_measurements', session_id=sid, track_id=tid)
        assert smooth['motion']['derivatives_smoothed'] and not smooth['motion']['positions_smoothed']
        assert all(r['vx'] is None for r in smooth['measurements'][:3])
        assert all(a['pixel_x'] == b['pixel_x'] for a, b in zip(rows, smooth['measurements']))
        saved = OUT / 'motion.tracker.json'
        exported = OUT / 'motion.csv'
        c.wait(c.mutate('save_project', sid, path=str(saved), overwrite=True))
        c.wait(c.mutate('export_csv', sid, track_id=tid, path=str(exported), overwrite=True))
        assert json.loads(saved.read_text())['version'] == 2
        csv_rows = list(csv.DictReader(exported.open()))
        for actual, row in zip(csv_rows, smooth['measurements']):
            near(float(actual['x_m']), row['x'])
            for column, key in [('vx_m_s', 'vx'), ('vy_m_s', 'vy'), ('speed_m_s', 'speed'), ('ax_m_s2', 'ax'), ('ay_m_s2', 'ay')]:
                assert (actual[column] == '') == (row[key] is None)
                if actual[column]: near(float(actual[column]), row[key])
            assert actual['derivative_method'] == 'local-quadratic-v1'
        reopened = c.wait(c.mutate('open_project', path=str(saved)))['session_id']
        restored = c.tool('get_measurements', session_id=reopened, track_id=tid)
        assert restored['measurements'] == smooth['measurements']
        assert restored['motion'] == smooth['motion']
        fixture['analysis']['tracks'][0]['points'].pop(30)
        gap_path = OUT / 'annotated-gap-v1.tracker.json'
        gap_path.write_text(json.dumps(fixture))
        gap_sid = c.wait(c.mutate('open_project', path=str(gap_path)))['session_id']
        gaps = c.tool('get_measurements', session_id=gap_sid, track_id=tid)
        assert gaps['total'] == 71
        assert all(gaps['measurements'][i]['derivative_unavailable_reason'] == 'gap_in_observations' for i in [29, 30])
        assert gaps['measurements'][28]['vx'] is not None
        rounded = fixture_project()
        for point in rounded['analysis']['tracks'][0]['points']:
            point['time'] = round(point['time'], 6)
        rounded_path = OUT / 'rounded-seconds-v1.tracker.json'
        rounded_path.write_text(json.dumps(rounded))
        rounded_sid = c.wait(c.mutate('open_project', path=str(rounded_path)))['session_id']
        rounded_rows = c.tool('get_measurements', session_id=rounded_sid,
                              track_id=rounded['analysis']['selectedTrackID'])['measurements']
        for i in range(1, 71):
            near(rounded_rows[i]['time'], i*1001/24000, 1e-12)
            near(rounded_rows[i]['vx'], rows[i]['vx']*100, 1e-6)
        c.mutate('select_session', reopened)
        report = dict(checks=['23 advertised tools', 'version-1 defaults', 'explicit calibration',
                              'axes origin and rotation', 'retry deduplication/conflict', 'stale mutation rejection',
                              'fractional timestamp finite differences', 'manual correction invalidation/undo',
                              'optional smoothing preserves raw positions', 'CSV/MCP equality', 'version-2 save/reopen',
                              'gaps stay unavailable', 'rational stamps override rounded legacy seconds'],
                      session_id=reopened, track_id=tid, measurements=smooth)
        (OUT / 'mcp-report.json').write_text(json.dumps(report, indent=2))
        print(json.dumps(dict(checks=report['checks'], session_id=reopened, track_id=tid)), flush=True)
    finally:
        c.close()


if __name__ == '__main__':
    main()
