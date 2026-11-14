"""Synthetic audit footage only; original artwork and exact analytic annotations."""
import json, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[1] / "build/audit/balltoss"
root.mkdir(parents=True, exist_ok=True)
W,H,N=1920,1080,72
rate_num,rate_den=24000,1001
base=bytearray(bytes([35,38,40])*(W*H))
# Supplied reference ruler: exactly 100 source pixels per metre.
for y in range(930,936):
 for x in range(1600,1701):
  i=(y*W+x)*3;base[i:i+3]=bytes([220]*3)
for x in range(1600,1701,10):
 for y in range(922,945):
  i=(y*W+x)*3;base[i:i+3]=bytes([220]*3)
for scenario in ('clean','occluded'):
 output=root/('synthetic-ball-toss-'+scenario+'.mp4')
 command=['ffmpeg','-hide_banner','-loglevel','error','-y','-f','rawvideo','-pixel_format','rgb24','-video_size',f'{W}x{H}','-framerate',f'{rate_num}/{rate_den}','-i','pipe:0','-an','-c:v','libx264','-preset','ultrafast','-crf','16','-bf','0','-pix_fmt','yuv420p','-metadata','title=SYNTHETIC ball toss audit fixture',str(output)]
 encoder=subprocess.Popen(command,stdin=subprocess.PIPE)
 rows=[]
 for frame in range(N):
  t=frame*rate_den/rate_num
  x=round(130+450*t);y=round(850-500*t+150*t*t)
  visible=not(scenario=='occluded' and 28<=frame<36)
  image=bytearray(base)
  if visible:
   for dy in range(-11,12):
    for dx in range(-11,12):
     if dx*dx+dy*dy<=121:
      value=235 if (dx+dy)%5<3 else 85
      i=((y+dy)*W+x+dx)*3;image[i:i+3]=bytes([value]*3)
  encoder.stdin.write(image)
  rows.append({'index':frame,'timestamp':{'value':frame*rate_den,'timescale':rate_num},'time_s':t,'pixel_x':x,'pixel_y':y,'visible':visible,'analytic_x':130+450*t,'analytic_y':850-500*t+150*t*t})
 encoder.stdin.close()
 if encoder.wait()!=0:raise RuntimeError('ffmpeg failed')
 annotations={'synthetic':True,'source_width':W,'source_height':H,'coordinate_origin':'top-left','fps':{'numerator':rate_num,'denominator':rate_den},'equations':{'x':'130 + 450*t','y':'850 - 500*t + 150*t*t'},'rasterization':'round centre to nearest source pixel','ball_radius_px':11,'calibration':{'first':[1600,933],'second':[1700,933],'known_distance':1,'unit':'m'},'frames':rows}
 output.with_suffix('.json').write_text(json.dumps(annotations,indent=2))
 print(output,flush=True)
(root/'README.md').write_text('Synthetic ball-toss audit fixtures, not camera footage or the upstream BallTossOut sample.\n1920x1080, 72 frames at 24000/1001 fps. Each JSON records the rational source timestamp, analytic and rasterized target coordinates, and visibility. The occluded variant hides frames 28 through 35. The ruler is a supplied synthetic 100 px = 1 m reference. No real-world accuracy claim follows from these fixtures.\n')
# Metadata-only variants exercise the media adapter without changing the trajectory.
clean=root/'synthetic-ball-toss-clean.mp4'
for name, extra in [('rotated',[]),('offset',[])]:
    command=['ffmpeg','-hide_banner','-loglevel','error','-y']
    if name=='offset':command+=['-itsoffset','0.5']
    if name=='rotated':command+=['-display_rotation','90']
    command+=['-i',str(clean),'-c','copy']+extra
    if name=='offset':command+=['-copyts']
    command+=[str(root/('synthetic-ball-toss-'+name+'.mp4'))]
    subprocess.run(command,check=True)
