import assert from 'node:assert/strict';
import { test, after } from 'node:test';
import { readFile, writeFile, mkdir, mkdtemp } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { existsSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';

// Focused offline real-browser test. Only native boundaries/settings are mocked.
const testDir = path.dirname(fileURLToPath(import.meta.url));
const S = path.resolve(process.env.DENOISE_LAYOUT_PROJECT || path.resolve(testDir, '../..'));
const requireFromProject = createRequire(path.join(S, 'package.json'));
const { build, stop } = requireFromProject('esbuild');
const { compile } = requireFromProject('@tailwindcss/node');
const E = process.env.DENOISE_LAYOUT_EVIDENCE || await mkdtemp(path.join(tmpdir(), 'rapidraw-denoise-layout-'));
const run = process.env.DENOISE_LAYOUT_RUN || 'candidate';
const out = process.env.DENOISE_LAYOUT_OUTPUT || path.join(E, 'browser-' + run);
if (existsSync(out)) throw new Error(`refusing to overwrite existing browser output: ${out}`);
await mkdir(out, { recursive: true });
const target = path.resolve(process.env.DENOISE_LAYOUT_SOURCE || path.join(S, 'src/components/modals/DenoiseModal.tsx'));
const locale = JSON.parse(await readFile(path.join(S, 'src/i18n/locales/en.json'), 'utf8'));
const entry = `
import React from 'react'; import {createRoot} from 'react-dom/client';
import DenoiseModal from ${JSON.stringify(target)};
const config=JSON.parse(decodeURIComponent(location.hash.slice(1)));
const calls=[];
const image='data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="3000"><rect width="4000" height="3000" fill="blue"/></svg>');
const props={isOpen:true,onClose:()=>calls.push(['close']),onDenoise:(...a)=>calls.push(['denoise',...a]),onBatchDenoise:async(...a)=>{calls.push(['batch',...a]);return [];},onSave:async()=>{calls.push(['save']);return '/synthetic-output.tif';},onOpenFile:p=>calls.push(['open',p]),error:null,previewBase64:null,originalBase64:null,isProcessing:false,progressMessage:null,aiModelDownloadStatus:null,isRaw:true,targetPaths:['/synthetic-input.raf']};
if(config.state==='preview')Object.assign(props,{previewBase64:image,originalBase64:image});
if(config.state==='processing')props.isProcessing=true;
if(config.state==='error')props.error='Synthetic denoise error';
if(config.state==='batch')props.targetPaths.push('/synthetic-second.raf');
if(config.state==='nonraw')props.isRaw=false;
createRoot(document.getElementById('root')).render(React.createElement(DenoiseModal,props));
const wait=ms=>new Promise(r=>setTimeout(r,ms));
const button=label=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===label);
function bounds(b){const r=b.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};}
function inViewport(b){if(!b)return false;const r=b.getBoundingClientRect();return r.width>0&&r.height>0&&r.left>=-0.5&&r.right<=innerWidth+0.5&&r.top>=-0.5&&r.bottom<=innerHeight+0.5;}
function reachable(b){if(!inViewport(b))return false;const r=b.getBoundingClientRect();const at=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);return !!at&&(at===b||b.contains(at));}
function fullyVisible(b){if(!inViewport(b))return false;const r=b.getBoundingClientRect();let clip={left:0,top:0,right:innerWidth,bottom:innerHeight};for(let p=b.parentElement;p;p=p.parentElement){const s=getComputedStyle(p);if(/(hidden|clip|auto|scroll)/.test(s.overflowX+' '+s.overflowY)){const q=p.getBoundingClientRect();clip={left:Math.max(clip.left,q.left),top:Math.max(clip.top,q.top),right:Math.min(clip.right,q.right),bottom:Math.min(clip.bottom,q.bottom)};}}return r.left>=clip.left-0.5&&r.top>=clip.top-0.5&&r.right<=clip.right+0.5&&r.bottom<=clip.bottom+0.5;}
async function inspect(){await wait(700);let label=config.state==='error'?'Close':config.state==='preview'?'Retry':config.state==='batch'?'Batch Denoise & Save':'Start';
 const b=button(label);const report={name:config.name,viewport:[innerWidth,innerHeight],label,bounds:b?bounds(b):null,reachable:reachable(b),checks:[]};
 const check=(name,ok)=>report.checks.push({name,ok});
 check(config.state==='processing'?'disabled action within viewport':'action within viewport and hit-testable',config.state==='processing'?inViewport(b):report.reachable);
 if(config.name==='unfolded-ready'){const resized=new Promise(resolve=>{const handle=e=>{if(e.source===parent&&e.data.denoiseResized){removeEventListener('message',handle);resolve();}};addEventListener('message',handle);});parent.postMessage({denoiseResize:true},'*');await resized;await wait(200);report.resizedBounds=bounds(b);check('same mounted dialog remains reachable after folding',innerWidth===360&&innerHeight===640&&reachable(b));}
 if(config.state==='processing'){check('processing action disabled',b?.disabled===true);b?.click();check('disabled action does not call denoise',calls.length===0);}
 else if(config.state==='error'){if(report.reachable)b.click();check('close handler',calls.some(c=>c[0]==='close'));}
 else if(config.state==='preview'){
   const save=button('Save');check('save reachable',reachable(save));if(reachable(save)){save.click();await wait(100);}
   const open=button('Open in Editor');check('saved open reachable',reachable(open));if(reachable(open))open.click();
   check('save/open callbacks preserve path',calls.some(c=>c[0]==='save')&&calls.some(c=>c[0]==='open'&&c[1]==='/synthetic-output.tif'));
 }else if(config.state==='batch'){if(report.reachable)b.click();await wait(50);check('batch passes both original paths',calls.some(c=>c[0]==='batch'&&c[3]?.length===2));}
 else if(config.state==='dropdown'){
   const trigger=document.querySelector('button[aria-haspopup="listbox"]');
   report.dropdownTrigger=trigger?bounds(trigger):null;
   check('method dropdown trigger reachable',reachable(trigger));
   if(reachable(trigger)){trigger.click();await wait(250);}
   const list=document.querySelector('[role="listbox"]');
   const options=list?[...list.querySelectorAll('[role="option"]')]:[];
   report.menu=list?bounds(list):null;
   report.options=options.map(o=>({label:o.textContent.trim(),bounds:bounds(o),fullyVisible:fullyVisible(o),reachable:reachable(o)}));
   check('method options opened',options.length>=2);
   const bm3d=options.find(o=>o.textContent.includes('BM3D'));
   check('BM3D option fully visible and hit-testable',!!bm3d&&fullyVisible(bm3d)&&reachable(bm3d));
   if(bm3d&&fullyVisible(bm3d)&&reachable(bm3d)){bm3d.click();await wait(150);}
   check('method selection updates trigger',document.querySelector('button[aria-haspopup="listbox"]')?.textContent.includes('BM3D')===true);
   const start=button('Start');if(reachable(start))start.click();
   check('selected method reaches real modal callback',calls.some(c=>c[0]==='denoise'&&c[2]==='bm3d'));
 }
 else{if(report.reachable)b.click();check('method/intensity preserved',calls.some(c=>c[0]==='denoise'&&c[1]===(config.state==='nonraw'?.15:.5)&&c[2]===(config.state==='nonraw'?'bm3d':'ai')));const cancel=button('Cancel');check('cancel reachable',reachable(cancel));if(reachable(cancel))cancel.click();check('cancel handler',calls.some(c=>c[0]==='close'));}
 parent.postMessage({denoiseLayout:report},'*');
}
inspect().catch(e=>parent.postMessage({denoiseLayout:{name:config.name,error:String(e)}},'*'));
`;
const bundle = await build({ absWorkingDir: S, nodePaths: [path.join(S, 'node_modules')], stdin: { contents: entry, resolveDir: S, loader: 'tsx' }, bundle: true, write: false, format: 'iife', platform: 'browser', jsx: 'automatic', metafile: true,
  plugins: [{ name: 'denoise-native-boundaries', setup(b) {
    b.onResolve({filter:/^react-i18next$/},()=>({path:'translation',namespace:'mock'}));
    b.onResolve({filter:/^@tauri-apps\/api\/(core|event)$/},a=>({path:a.path,namespace:'mock'}));
    b.onResolve({filter:/\/store\/useSettingsStore$/},()=>({path:'settings',namespace:'mock'}));
    b.onLoad({filter:/.*/,namespace:'mock'},a=>({loader:'js',contents:a.path==='translation'?`const en=${JSON.stringify(locale)};export const useTranslation=()=>({t:key=>key.split('.').reduce((v,k)=>v?.[k],en)??key});`:a.path==='settings'?`export const useSettingsStore=s=>s({appSettings:{aiProvider:'local'}});`:a.path.endsWith('core')?`export const invoke=async()=>false;`:`export const listen=async()=>()=>{};`}));
    b.onResolve({filter:/^\./},a=>{
      const candidateDropdown=path.resolve(path.dirname(target),'../ui/Dropdown.tsx');
      if(a.importer===target&&a.path==='../ui/Dropdown'&&existsSync(candidateDropdown))return {path:candidateDropdown};
      const base=a.importer===candidateDropdown?path.resolve(S,'src/components/ui',a.path):path.resolve(S,'src/components/modals',a.path);
      const resolved=[base,base+'.tsx',base+'.ts',base+'.js'].find(existsSync);if(resolved)return {path:resolved};
    });
  }}] });
after(stop);
const sources = await Promise.all(Object.keys(bundle.metafile.inputs).filter(p=>!p.includes('node_modules')&&!p.includes('<stdin>')&&!p.startsWith('mock:')).map(p=>readFile(path.isAbsolute(p)?p:path.join(S,p),'utf8')));
const cssCompiler=await compile(await readFile(path.join(S,'src/styles.css'),'utf8'),{base:path.join(S,'src'),onDependency:()=>{}});
const css=cssCompiler.build(sources.join('\n').match(/[^\s"'`<>{}=]+/g)||[]);
const csp=`<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-src file:;">`;
await writeFile(path.join(out,'case.html'),`<!doctype html><html><head>${csp}<meta name="viewport" content="width=device-width,initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script>${bundle.outputFiles[0].text.replaceAll('</script','<\\/script')}</script></body></html>`);
const cases=[{name:'folded-ready',width:360,height:640,state:'initial'},{name:'folded-preview-save',width:384,height:832,state:'preview'},{name:'folded-processing-disabled',width:360,height:640,state:'processing'},{name:'folded-batch',width:360,height:640,state:'batch'},{name:'short-landscape-close',width:640,height:360,state:'error'},{name:'unfolded-ready',width:844,height:768,state:'initial'},{name:'desktop-preview-save',width:1280,height:800,state:'preview'},{name:'desktop-nonraw-method',width:1280,height:800,state:'nonraw'},{name:'folded-method-dropdown',width:360,height:640,state:'dropdown'},{name:'short-landscape-method-dropdown',width:640,height:360,state:'dropdown'}];
// Actual iframe viewports exercise the browser's media/flex/scroll layout engine.
const driver=`const cases=${JSON.stringify(cases)};const results=[];const frames=[];addEventListener('message',e=>{if(!frames.includes(e.source))return;if(e.data.denoiseResize){const frame=[...document.querySelectorAll('iframe')].find(f=>f.contentWindow===e.source);frame.style.width='360px';frame.style.height='640px';e.source.postMessage({denoiseResized:true},'*');return;}if(!e.data.denoiseLayout)return;results.push(e.data.denoiseLayout);if(results.length===cases.length){results.sort((a,b)=>cases.findIndex(c=>c.name===a.name)-cases.findIndex(c=>c.name===b.name));const pre=document.createElement('pre');pre.id='results';pre.textContent=JSON.stringify(results);document.body.replaceChildren(pre);}});for(const c of cases){const frame=document.createElement('iframe');frame.style.width=c.width+'px';frame.style.height=c.height+'px';frame.src='./case.html#'+encodeURIComponent(JSON.stringify(c));document.body.appendChild(frame);frames.push(frame.contentWindow);}`;
await writeFile(path.join(out,'index.html'),`<!doctype html><html><head>${csp}</head><body><script>${driver}</script></body></html>`);
const chrome=path.resolve(process.env.DENOISE_LAYOUT_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome');
const argv=['--headless','--disable-gpu','--no-first-run','--no-default-browser-check','--disable-background-networking','--disable-component-update','--disable-sync','--disable-extensions','--host-resolver-rules=MAP * 0.0.0.0','--allow-file-access-from-files',`--user-data-dir=${out}/chrome-profile`,'--virtual-time-budget=12000','--dump-dom','file://'+path.join(out,'index.html')];
const sourceSha256=createHash('sha256').update(await readFile(target)).digest('hex');
await writeFile(path.join(out,'browser-launch.json'),JSON.stringify({targetSource:path.resolve(target),targetSourceSha256:sourceSha256,projectRoot:S,chrome,argv:[chrome,...argv]},null,2)+'\n');
assert.ok(existsSync(chrome),`configured Chrome executable does not exist: ${chrome}`);
let response;
try{response=await promisify(execFile)(chrome,argv,{timeout:90000,maxBuffer:4*1024*1024});}catch(e){await writeFile(path.join(out,'browser-failure.json'),JSON.stringify({message:e.message,code:e.code,killed:e.killed},null,2));throw e;}
await writeFile(path.join(out,'dom.html'),response.stdout);await writeFile(path.join(out,'chrome.stderr'),response.stderr);
const match=response.stdout.match(/<pre id="results">(\[[\s\S]*?\])<\/pre>/);
assert.ok(match,'browser must finish all actual mounted cases');
const results=JSON.parse(match[1].replaceAll('&quot;','"').replaceAll('&amp;','&'));
await writeFile(path.join(out,'results.json'),JSON.stringify(results,null,2)+'\n');
assert.equal(results.length,cases.length);
for(const result of results)test(result.name,()=>{assert.equal(result.error,undefined);for(const check of result.checks)assert.ok(check.ok,check.name+' '+JSON.stringify(result.bounds));});
