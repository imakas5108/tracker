"""Release app benchmark using synthetic videos and existing stdio test client."""
import importlib.util, json, os, subprocess, time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/os.environ.get('TRACKER_BENCHMARK_OUTPUT','build/audit/resolution');OUT.mkdir(parents=True,exist_ok=True)
spec=importlib.util.spec_from_file_location('mcp_client',ROOT/'scripts/test-mcp.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

def percentile(values, fraction):
    values=sorted(values);return values[min(len(values)-1,round((len(values)-1)*fraction))]

def main():
    c=m.Client();report=[]
    try:
        source=ROOT/'build/audit/balltoss/synthetic-ball-toss-clean.mp4'
        for count in [1,2,4,8]:
            sessions=[]
            for i in range(count):
                path=OUT/f'balltoss-{count}-{i}.mp4'
                if not path.exists():os.link(source,path)
                s=c.wait(c.mutate('import_video',path=str(path)))['session_id']
                tid=c.mutate('add_track',s,name=f'Synthetic workload {count}/{i+1}')['track_id']
                c.wait(c.mutate('seek',s,time=0))
                c.mutate('mark_point',s,track_id=tid,time=0,x=130,y=850)
                c.mutate('configure_tracking',s,template_radius=12,search_radius=64,minimum_score=.9,minimum_separation=.015)
                sessions.append((s,tid))
            jobs=[];began=time.monotonic()
            for sid,tid in sessions:jobs.append(c.mutate('start_tracking',sid,track_id=tid))
            times=[];states={};peak=0;queued=False
            pid=os.environ.get('TRACKER_BENCHMARK_PID') or subprocess.check_output(['pgrep','-x','Tracker'],text=True).splitlines()[-1]
            while len(states)<count:
                for job in jobs:
                    if job['job_id'] in states:continue
                    start=time.monotonic();status=c.tool('get_job',job_id=job['job_id']);times.append((time.monotonic()-start)*1000)
                    queued |= status['state']=='queued'
                    if status['state'] in ['completed','failed','stopped','awaiting_review']:
                        assert status['state']=='completed',status;states[job['job_id']]=status
                rss=int(subprocess.check_output(['ps','-p',pid,'-o','rss='],text=True).strip())*1024;peak=max(peak,rss)
                time.sleep(.01)
            elapsed=time.monotonic()-began
            for sid,tid in sessions:
                assert c.tool('get_measurements',session_id=sid,track_id=tid)['total']==72
                c.wait(c.mutate('save_project',sid,path=str(OUT/f'{sid}.tracker.json'),overwrite=True))
            row={'videos':count,'active_slots':1,'frames':71*count,'wall_s':elapsed,'frames_per_s':71*count/elapsed,'mcp_rtt_p95_ms':percentile(times,.95),'mcp_rtt_max_ms':max(times),'sampled_app_peak_rss_bytes':peak,'queued_observed':queued}
            report.append(row);print(json.dumps(row),flush=True)
        # Measure cancellation acknowledgement and the next queued job's admission.
        s,tid=sessions[0];c.wait(c.mutate('seek',s,time=0));running=c.mutate('start_tracking',s,track_id=tid)
        s2,t2=sessions[1];c.wait(c.mutate('seek',s2,time=0));waiting=c.mutate('start_tracking',s2,track_id=t2)
        start=time.monotonic();cancel=c.mutate('cancel_job',job_id=waiting['job_id']);ack=(time.monotonic()-start)*1000
        start=time.monotonic();c.mutate('cancel_job',job_id=running['job_id']);running_ack=(time.monotonic()-start)*1000
        record={'queued_cancel_ack_ms':ack,'queued_state':cancel['state'],'running_cancel_ack_ms':running_ack}
        for sid,tid in sessions:
            c.wait(c.mutate('save_project',sid,path=str(OUT/f'{sid}.tracker.json'),overwrite=True))
        (OUT/'workload-report.json').write_text(json.dumps({'workloads':report,'cancellation':record},indent=2))
        print(json.dumps(record),flush=True)
        pressure=subprocess.run(['memory_pressure','-Q'],text=True,capture_output=True)
        (OUT/'memory-pressure.txt').write_text(pressure.stdout+pressure.stderr)
    finally:c.close()
if __name__=='__main__':main()
