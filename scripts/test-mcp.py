#!/usr/bin/env python3
"""Exercise the real packaged stdio bridge against Tracker; writes only build/mcp-e2e/."""
import base64
import json
import queue
import struct
import subprocess
import threading
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'build/mcp-e2e'

class Client:
    def __init__(self):
        self.messages = queue.Queue()
        self.serial = 0
        self.stderr = (OUT / ('bridge-' + uuid.uuid4().hex + '.log')).open('w')
        self.process = subprocess.Popen([str(ROOT / 'build/Tracker.app/Contents/MacOS/TrackerMCP')], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, text=True, bufsize=1)
        def read():
            for line in self.process.stdout:
                try: self.messages.put(json.loads(line))
                except json.JSONDecodeError: self.messages.put({'invalid_stdout': line})
        threading.Thread(target=read, daemon=True).start()
        initialized = self.rpc('initialize', {'protocolVersion':'2025-11-25', 'clientInfo':{'name':'tracker-e2e','version':'1'},'capabilities':{'experimental':{'codex/auth-change':{}},'elicitation':{'form':{},'url':{}}}})
        assert 'result' in initialized, initialized
        self.initialized = initialized['result']
        self.send({'jsonrpc':'2.0','method':'notifications/initialized'})
    def send(self, value):
        self.process.stdin.write(json.dumps(value) + '\n'); self.process.stdin.flush()
    def rpc(self, method, params):
        self.serial += 1
        self.send({'jsonrpc':'2.0','id':self.serial,'method':method,'params':params})
        until = time.monotonic() + 40
        while time.monotonic() < until:
            value = self.messages.get(timeout=max(.1, until-time.monotonic()))
            assert 'invalid_stdout' not in value, value
            if value.get('id') == self.serial: return value
        raise TimeoutError(method)
    def raw(self, tool_name, **args):
        value = self.rpc('tools/call', {'name':'tracker_'+tool_name,'arguments':args})
        assert 'result' in value, value
        return value['result']
    def tool(self, tool_name, **args):
        result = self.raw(tool_name, **args)
        assert not result.get('isError'), (tool_name,result)
        return result['structuredContent']
    def mutate(self, tool_name, session=None, **args):
        if session:
            state = self.tool('get_session',session_id=session)
            args.update(session_id=session, expected_revision=state['revision'])
        args.setdefault('request_id',str(uuid.uuid4()))
        return self.tool(tool_name,**args)
    def wait(self, operation, states=('completed',), timeout=40):
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            status = self.tool('get_job',job_id=operation['job_id'])
            if status['state'] in states: return status
            assert status['state'] not in ('failed','cancelled','stopped'), status
            time.sleep(.025)
        raise TimeoutError(operation)
    def close(self):
        self.process.stdin.close()
        try: self.process.wait(timeout=5)
        except subprocess.TimeoutExpired: self.process.terminate(); self.process.wait(timeout=5)
        self.stderr.close()

def main():
    OUT.mkdir(parents=True,exist_ok=True)
    c = Client()
    try:
        tools = c.rpc('tools/list',{})['result']['tools']
        assert len(tools) == 21, len(tools)
        imported = c.wait(c.mutate('import_video',path=str(ROOT / 'build/fixtures/clean.mov')))
        sid = imported['session_id']
        start = c.tool('get_session',session_id=sid)
        frame = c.raw('get_frame',session_id=sid,time=.12,crop={'x':20,'y':30,'width':80,'height':60},max_edge=40)
        assert not frame.get('isError'), frame
        image = next(x for x in frame['content'] if x['type']=='image')
        png = base64.b64decode(image['data']); assert png[:8] == b'\x89PNG\r\n\x1a\n'
        assert struct.unpack('>II',png[16:24]) == (40,30)
        (OUT/'crop.png').write_bytes(png)
        assert frame['structuredContent']['mapping']['scale_x'] == .5
        assert frame['structuredContent']['time'] == .12
        assert c.tool('get_session',session_id=sid)['time'] == start['time']
        c.mutate('set_calibration',sid,x1=10,y1=10,x2=110,y2=10,length=1,unit='m')
        state = c.tool('get_session',session_id=sid)
        args = dict(session_id=sid,expected_revision=state['revision'],request_id=str(uuid.uuid4()),name='MCP synthetic target')
        added = c.tool('add_track',**args)
        assert c.tool('add_track',**args) == added
        conflict = c.raw('add_track',**dict(args,name='conflict'))
        assert conflict['structuredContent']['code'] == 'request_id_conflict'
        stale = c.raw('add_track',**dict(args,request_id=str(uuid.uuid4())))
        assert stale['structuredContent']['code'] == 'stale_revision'
        tid = added['track_id']
        c.wait(c.mutate('seek',sid,time=0))
        c.mutate('mark_point',sid,track_id=tid,time=0,x=30,y=45)
        c.mutate('configure_tracking',sid,template_radius=8,search_radius=24,minimum_score=.90,minimum_separation=.015)
        job = c.mutate('start_tracking',sid,track_id=tid)
        # End the stdio connection; the app retains and completes its own job.
        c.close(); c = Client()
        completed = c.wait(job)
        rows = c.tool('get_measurements',session_id=sid,track_id=tid,offset=0,limit=30)
        assert rows['total']==60, rows
        later = c.tool('get_measurements',session_id=sid,track_id=tid,offset=30,limit=30,expected_revision=rows['revision'])
        assert len(later['measurements'])==30
        assert rows['measurements'][0]['x'] == .3
        assert rows['measurements'][0]['y'] == .75
        saved = c.wait(c.mutate('save_project',sid,path=str(OUT/'clean.tracker.json'),overwrite=True))
        exported = c.wait(c.mutate('export_csv',sid,track_id=tid,path=str(OUT/'clean.csv'),overwrite=True))
        exists = c.mutate('save_project',sid,path=str(OUT/'clean.tracker.json'))
        rejected = c.wait(exists,states=('failed',))
        assert rejected['error']['code']=='destination_exists'
        reopened = c.wait(c.mutate('open_project',path=str(OUT/'clean.tracker.json')))
        assert reopened['result']['tracks'][0]['count']==60
        review_import = c.wait(c.mutate('import_video',path=str(ROOT/'build/fixtures/occlusion.mov')))
        rsid = review_import['session_id']
        rt = c.mutate('add_track',rsid,name='Occlusion review')['track_id']
        c.wait(c.mutate('seek',rsid,time=0))
        c.mutate('mark_point',rsid,track_id=rt,time=0,x=30,y=45)
        c.mutate('configure_tracking',rsid,template_radius=8,search_radius=24,minimum_score=.9,minimum_separation=.015)
        uncertain = c.wait(c.mutate('start_tracking',rsid,track_id=rt),states=('awaiting_review',))
        review = c.tool('get_review',session_id=rsid)
        assert review['pending'] and not review['recorded'] and review['time']==1.2, review
        assert c.tool('get_measurements',session_id=rsid,track_id=rt)['total']==20
        evidence = c.raw('get_frame',session_id=rsid,time=review['time'],overlays=True)
        (OUT/'uncertain.png').write_bytes(base64.b64decode(next(x['data'] for x in evidence['content'] if x['type']=='image')))
        # Wait for the native review seek; candidate acceptance is never automatic.
        for _ in range(200):
            if not c.tool('get_session',session_id=rsid)['seeking']: break
            time.sleep(.01)
        continuation = c.mutate('review_match',rsid,job_id=uncertain['job_id'],action='skip')
        c.wait(continuation,states=('awaiting_review','completed'))
        c.mutate('cancel_job',job_id=continuation['job_id'])
        assert c.tool('get_measurements',session_id=rsid,track_id=rt)['total']==20
        c.wait(c.mutate('save_project',rsid,path=str(OUT/'occlusion.tracker.json'),overwrite=True))
        # Relinking is explicit, with no native chooser.
        document = json.loads((OUT/'clean.tracker.json').read_text()); document['source']='file:///no-such-tracker-footage.mov'
        (OUT/'missing.tracker.json').write_text(json.dumps(document))
        missing = c.wait(c.mutate('open_project',path=str(OUT/'missing.tracker.json')),states=('failed',))
        assert missing['error']['code']=='relink_required'
        c.wait(c.mutate('open_project',path=str(OUT/'missing.tracker.json'),relink_path=str(ROOT/'build/fixtures/clean.mov')))
        c.mutate('select_session',rsid)
        report = dict(initialization=c.initialized,tool_count=len(tools),clean_job=completed,save=saved,export=exported,review=review,checks=['stdio initialization','native import','PNG delivery/crop mapping','revision rejection','retry deduplication/conflict','explicit calibration','seed and autotracking','reconnect preserves jobs','paginated measurements','no-clobber save','CSV export','project reopen','uncertain evidence stays unrecorded','explicit skip','cancel pending review','missing source relink error','explicit relink'])
        (OUT/'report.json').write_text(json.dumps(report,indent=2))
        print(json.dumps({'passed':len(report['checks']),'report':str(OUT/'report.json'),'review_session_id':rsid}))
    finally: c.close()

if __name__=='__main__': main()
