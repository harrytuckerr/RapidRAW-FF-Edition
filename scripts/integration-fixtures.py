"""Synthetic camera/RAW/profile fixtures, generated without vendor assets."""
import struct,json
from pathlib import Path
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir',type=Path,required=True)
P=parser.parse_args().output_dir
P.mkdir(parents=True,exist_ok=True)
S={1:1,2:1,3:2,4:4,5:8,10:8,11:4}
def tag(n,t,v):
 if t==2:b=v.encode()+b'\0';count=len(b)
 elif t in (5,10):b=b''.join(struct.pack('<'+('ii' if t==10 else 'II'),*x) for x in v);count=len(v)
 else:b=struct.pack('<'+{1:'B',3:'H',4:'I',11:'f'}[t]*len(v),*v);count=len(v)
 return [n,t,count,b]
def tiff(path,tags,pixels=b'',magic=42):
 tags.sort();offset=8+2+len(tags)*12+4;extra=bytearray();entries=[]
 for n,t,c,b in tags:
  if len(b)>4:
   if (offset+len(extra))%2:extra+=b'\0'
   data=struct.pack('<I',offset+len(extra));extra+=b
  else:data=b.ljust(4,b'\0')
  entries.append((n,struct.pack('<HHI',n,t,c)+data))
 pixel_offset=offset+len(extra)
 entries=[(n,e[:8]+struct.pack('<I',pixel_offset) if n==273 else e) for n,e in entries]
 path.write_bytes(b'II'+struct.pack('<HIH',magic,8,len(tags))+b''.join(e for _,e in entries)+b'\0'*4+extra+pixels)
I=[(1,1),(0,1),(0,1),(0,1),(1,1),(0,1),(0,1),(0,1),(1,1)]
camera='RapidRAW Integration Camera'
w=h=64
pix=[]
for y in range(h):
 for x in range(w):
  rgb=(0.05+0.6*x/(w-1),0.05+0.6*y/(h-1),0.2+0.2*(x+y)/(w+h-2))
  c=[[0,1],[1,2]][y%2][x%2]
  pix.append(round(64+rgb[c]*[.5,1,.75][c]*4031))
tags=[tag(256,4,[w]),tag(257,4,[h]),tag(258,3,[16]),tag(259,3,[1]),tag(262,3,[32803]),tag(271,2,'RapidRAW'),tag(272,2,'Integration Camera'),tag(273,4,[0]),tag(274,3,[1]),tag(277,3,[1]),tag(278,4,[h]),tag(279,4,[w*h*2]),tag(284,3,[1]),tag(33421,3,[2,2]),tag(33422,1,[0,1,1,2]),tag(50706,1,[1,4,0,0]),tag(50707,1,[1,1,0,0]),tag(50708,2,camera),tag(50710,1,[0,1,2]),tag(50711,3,[1]),tag(50713,3,[1,1]),tag(50714,5,[(64,1)]),tag(50717,4,[4095]),tag(50721,10,I),tag(50728,5,[(1,2),(1,1),(3,4)]),tag(50778,3,[21]),tag(50829,4,[0,0,h,w])]
tiff(P/'synthetic-camera.dng',tags,struct.pack('<'+'H'*len(pix),*pix))
tiff(P/'synthetic-base.dcp',[tag(50708,2,camera),tag(50721,10,I),tag(50778,3,[21]),tag(50936,2,'Integration Test Base'),tag(50941,4,[2]),tag(51109,10,[(1,1)])],magic=0x4352)
(P/'neutral.json').write_text('{}\n')
print('Created synthetic CFA DNG and DCP in',P)
# A second base gives bounded ProPhoto test pixels for native Look evaluation.
fm=[(.7977),(.1352),(.0313),(.2880),(.7119),(.0001),0.,0.,(.8251)]
tiff(P/'synthetic-look-base.dcp',[tag(50708,2,camera),tag(50721,10,I),tag(50778,3,[21]),tag(50936,2,'Integration Look Base'),tag(50941,4,[2]),tag(50964,10,[(round(v*1000000),1000000) for v in fm])],magic=0x4352)
# A tiny RGB swap table exercises native acquisition + evaluator without vendor data.
import zlib
alphabet="0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#"
def adobe85(b):
 out=''
 for offset in range(0,len(b),4):
  chunk=b[offset:offset+4];v=int.from_bytes(chunk,'little');digits=''
  for _ in range(len(chunk)+1):digits+=alphabet[v%85];v//=85
  out+=digits
 return out
samples=[]
for r in range(2):
 for g in range(2):
  for b in range(2):
   nop=lambda i:(i*65535+(2>>1))//(2-1)
   desired=[g*65535,r*65535,b*65535]
   samples.extend((v-nop(i))&65535 for v,i in zip(desired,[r,g,b]))
payload=struct.pack('<IIII',1,1,3,2)+struct.pack('<'+'H'*len(samples),*samples)+struct.pack('<III',2,2,0)+struct.pack('<dd',0.,2.)
blob=adobe85(struct.pack('<I',len(payload))+zlib.compress(payload))
(P/'synthetic-look.xmp').write_text('''<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:PresetType="Look" crs:Cluster="Synthetic" crs:ProcessVersion="15.4" crs:UUID="INTEGRATION-SYNTHETIC-LOOK" crs:CameraProfile="Integration Look Base" crs:SupportsAmount="True" crs:RGBTable="INTEGRATION" crs:Table_INTEGRATION="'''+blob+'''"><crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Synthetic channel swap</rdf:li></rdf:Alt></crs:Name></rdf:Description></rdf:RDF></x:xmpmeta>''')
print('Created bounded base and synthetic RGB Look')
