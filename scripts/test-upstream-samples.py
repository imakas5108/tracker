"""Exercise user-supplied Tracker samples through the packaged local MCP bridge.

Media stays under build/; no upstream media is redistributed by this script.
"""
import argparse,base64,hashlib,importlib.util,json,os,re,time,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('client',ROOT/'scripts/test-mcp.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
# Source-pixel points selected visually, not ground-truth annotations.
SEEDS={
 'BallDrop':(8,199,93,'falling ball; interlaced field images'),
 'BallTossOut':(12,174,154,'ball centre'),
 'BallTossUp':(20,226,166,'ball centre; interlaced field images'),
 'BlockOnCart':(3,222,105,'upper left block corner'),
 'CupsClips':(3,177,37,'NEGATIVE: accumulated strobe images, first cup'),
 'FlexTrackRaces':(10,59,106,'lower ball; video contains trial changes'),
 'Pendulum':(3,57,178,'bob centre'),
 'PucksCollide':(3,70,118,'blue puck centre'),
 'PucksCollideSequence':(3,159,104,'blue puck; multiple edited trials'),
 'SHMScale':(3,132,99,'hanging mass centre'),
 'TippingCylinders':(3,145,120,'cylinder centre; appearance rotates'),
 'TossOutStrobe':(7,66,106,'NEGATIVE: accumulated strobe images, first ball'),
 'WhichLandsFirst':(3,268,61,'right ball centre')}

def prepare_samples():
 out=ROOT/'build/upstream-samples';out.mkdir(parents=True,exist_ok=True);manifest={}
 for name in ['mechanics_videos.zip','spectroscopy_videos.zip']:
  archive=ROOT/name
  if not archive.exists():raise FileNotFoundError('Supply '+name+' in the checkout; upstream media is not bundled.')
  with zipfile.ZipFile(archive) as z:
   files=[]
   for member in z.infolist():
    target=(out/member.filename).resolve()
    if not target.is_relative_to(out.resolve()):raise ValueError('Archive path escapes sample directory')
    if member.is_dir():continue
    if target.suffix not in ['.mp4','.jpg']:raise ValueError('Unexpected sample type: '+member.filename)
    payload=z.read(member);target.parent.mkdir(parents=True,exist_ok=True)
    if target.exists():
     if target.read_bytes()!=payload:raise ValueError('Existing sample differs: '+str(target))
    else:target.write_bytes(payload)
    files.append(member.filename)
   manifest[name]=dict(sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),files=files)
 (out/'archives.json').write_text(json.dumps(manifest,indent=2))

def main():
 p=argparse.ArgumentParser();p.add_argument('--runtime',required=True);p.add_argument('--phase',default='baseline');p.add_argument('--method',choices=['ssd','zncc']);p.add_argument('--score',type=float,default=.9);p.add_argument('--radius',type=int,default=12);p.add_argument('--mechanics-only',action='store_true');a=p.parse_args()
 if not re.fullmatch('[a-zA-Z0-9_-]+',a.phase):p.error('phase must be a simple output label')
 if not 3<=a.radius<=48 or not 0<=a.score<=1:p.error('invalid template radius or score')
 prepare_samples()
 out=ROOT/'build/upstream-results'/a.phase;out.mkdir(parents=True,exist_ok=True);m.OUT.mkdir(parents=True,exist_ok=True)
 c=m.Client(env=dict(os.environ,TRACKER_CONTROL_DIRECTORY=a.runtime));results=[]
 def save(): (out/'report.json').write_text(json.dumps(results,indent=2))
 def frame(sid,t,name):
  before=time.monotonic();r=c.raw('get_frame',session_id=sid,time=t,overlays=True)
  if r.get('isError'):return r
  (out/(name+'.png')).write_bytes(base64.b64decode(next(x['data'] for x in r['content'] if x['type']=='image')))
  return dict(r['structuredContent'],reply_seconds=time.monotonic()-before)
 try:
  files=sorted((ROOT/'build/upstream-samples/mechanics_videos').glob('*'))
  files=[x for x in files if x.suffix in ['.mp4','.jpg']]
  if not a.mechanics_only:files+=sorted((ROOT/'build/upstream-samples/spectroscopy_videos').glob('*.mp4'))
  for path in files:
   entry={'file':str(path.relative_to(ROOT))};results.append(entry)
   try:
    before=time.monotonic();op=c.mutate('import_video',path=str(path));entry['import_ack_s']=time.monotonic()-before
    ready=c.wait(op,states=('completed','failed'));entry['import_ready_s']=time.monotonic()-before;entry['import']=ready
    if ready['state']=='failed':save();print(json.dumps(dict(file=path.name,result='import_failed')),flush=True);continue
    sid=ready['session_id'];state=c.tool('get_session',session_id=sid);entry['source']=state
    entry['first_frame']=frame(sid,0,path.stem+'-first')
    if path.stem not in SEEDS: save();continue
    index,x,y,label=SEEDS[path.stem];entry['seed_description']=label
    actual=frame(sid,index/30,path.stem+'-seed');entry['seed_frame']=actual
    tid=c.mutate('add_track',sid,name=a.phase+' '+label)['track_id'];entry['track_id']=tid
    c.wait(c.mutate('seek',sid,time=actual['time']));c.mutate('mark_point',sid,track_id=tid,time=actual['time'],x=x,y=y)
    config=dict(template_radius=a.radius,search_radius=64,minimum_score=a.score,minimum_separation=.015)
    if a.method:config['matching_method']=a.method
    entry['configuration']=config;c.mutate('configure_tracking',sid,**config)
    before=time.monotonic();job=c.mutate('start_tracking',sid,track_id=tid);entry['job']=job;poll=[]
    while True:
     t=time.monotonic();status=c.tool('get_job',job_id=job['job_id']);poll.append(dict(elapsed=time.monotonic()-before,reply_s=time.monotonic()-t,status=status))
     if status['state'] in ['completed','awaiting_review','failed','stopped','cancelled']:break
     if time.monotonic()-before>90: c.mutate('cancel_job',job_id=job['job_id']);break
     time.sleep(.05)
    entry['poll']=poll;entry['review']=c.tool('get_review',session_id=sid)
    if entry['review'].get('pending'):entry['review_frame']=frame(sid,entry['review']['time'],path.stem+'-review')
    rows=c.tool('get_measurements',session_id=sid,track_id=tid,limit=500);entry['measurements']=rows
    while rows.get('next_offset') is not None:
     rows=c.tool('get_measurements',session_id=sid,track_id=tid,offset=rows['next_offset'],limit=500,expected_revision=entry['measurements']['revision']);entry['measurements']['measurements']+=rows['measurements']
    entry['save']=c.wait(c.mutate('save_project',sid,path=str(out/(path.stem+'.tracker.json')),overwrite=True))
    entry['csv']=c.wait(c.mutate('export_csv',sid,track_id=tid,path=str(out/(path.stem+'.csv')),overwrite=True))
    if entry['review'].get('pending'):c.mutate('cancel_job',job_id=job['job_id'])
    print(json.dumps(dict(file=path.name,state=status['state'],processed=status.get('processed_frames'),observations=entry['measurements']['total'],review=entry['review'].get('reason'))),flush=True)
   except Exception as e:entry['exception']=str(e);print(path.name,str(e),flush=True)
   save()
 finally:c.close()
 unexpected=[r for r in results if 'exception' in r or (r['file'].endswith('.mp4') and r.get('import',{}).get('state')!='completed')]
 print(json.dumps(dict(inputs=len(results),imported_videos=sum(r.get('import',{}).get('state')=='completed' for r in results),tracking_runs=sum('job' in r for r in results),unexpected_errors=len(unexpected))),flush=True)
 if unexpected:raise SystemExit(1)
if __name__=='__main__':main()
