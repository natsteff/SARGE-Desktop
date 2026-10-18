const {test}=require('node:test');
const assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
const fs=require('node:fs');
const path=require('node:path');
const root=path.resolve(__dirname,'../ui');
const source={id:'s',name:'Birds',drive_name:'Blue USB',description:'Test source',path:'G:\\20261017 - Birds',volume_id:'volume-1',volume_label:'Photos',drive_letter:'G:',folder_date:'2026-10-17',online:true,archived:false,last_scan:'2026-10-17T12:00:00Z',last_complete:'2026-10-17T12:00:00Z'};
const tick=()=>new Promise(resolve=>setTimeout(resolve,20));
async function boot(t,overrides={}){
 const dom=new JSDOM(fs.readFileSync(path.join(root,'index.html'),'utf8'),{url:'http://tauri.localhost',runScripts:'outside-only'});t.after(()=>dom.window.close());
 dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true};dom.window.HTMLDialogElement.prototype.close=function(){this.open=false};
 const calls=[];const handlers={get_state:()=>({sources:[source],files:[],jobs:[],busy:false,catalog_path:'test/catalog.db'}),source_candidates:()=>[],save_source:()=> 's',start_jobs:()=>({queued:1}),...overrides};
 dom.window.__TAURI__={core:{invoke:async(command,args)=>{calls.push({command,args});if(!handlers[command])throw Error('Unexpected command '+command);return handlers[command](args)}}};
 dom.window.eval(fs.readFileSync(path.join(root,'theme.js'),'utf8'));dom.window.eval(fs.readFileSync(path.join(root,'app.js'),'utf8'));await tick();
 return {w:dom.window,d:dom.window.document,calls};
}
test('native bridge initializes, navigation works and theme persists',async t=>{
 const {w,d,calls}=await boot(t);assert.match(d.querySelector('#desktopInfo').textContent,/catalog.db/);assert.equal(d.querySelector('#scanAllButton').disabled,false);
 d.querySelector('[data-view="drives"]').click();assert.equal(d.querySelector('#heading').textContent,'Drives & locations');assert.match(d.querySelector('#driveCards').textContent,/Blue USB/);
 d.querySelector('#themePicker').value='soldiers';d.querySelector('#themePicker').dispatchEvent(new w.Event('change'));assert.equal(d.documentElement.dataset.theme,'soldiers');assert.equal(w.localStorage.getItem('sarge-theme'),'soldiers');assert.equal(calls[0].command,'get_state');
});
test('all three job modes use native start_jobs and copy confirms before starting',async t=>{
 for(const mode of ['scan','scan-copy','copy']){
  const {w,d,calls}=await boot(t,{pick_folder:()=>({path:'D:\\destination'})});d.querySelector('[data-job]').click();
  const select=d.querySelector('#jobMode');select.value=mode;select.dispatchEvent(new w.Event('change'));
  assert.equal(d.querySelector('#destinations').hidden,mode==='scan');
  const submit=()=>d.querySelector('#jobForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));
  if(mode!=='scan'){submit();await tick();assert.match(d.querySelector('#jobError').textContent,/at least one destination/);d.querySelector('#matchedPath').value='D:\\matched';d.querySelector('#unmatchedPath').value='D:\\unmatched';}
  submit();await tick();if(mode!=='scan'){assert.ok(!calls.some(c=>c.command==='start_jobs'));d.querySelector('#confirmCopy').click();await tick()}const request=calls.find(c=>c.command==='start_jobs').args.request;assert.equal(request.mode,mode);assert.deepEqual([...request.source_ids],['s']);assert.equal(d.querySelector('#heading').textContent,'Scan activity');
 }
});
test('duplicate source review forwards explicit update choice',async t=>{
 const {w,d,calls}=await boot(t,{source_candidates:()=>[source]});d.querySelector('[data-command="addSource"]').click();
 d.querySelector('[name=path]').value='G:\\20261017 - Birds';d.querySelector('[name=name]').value='Birds';d.querySelector('#nativeSourceForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));await tick();
 assert.match(d.querySelector('#dialogContent').textContent,/An existing source may match/);assert.ok(!calls.some(c=>c.command==='save_source'));d.querySelector('[data-update]').click();await tick();
 const draft=calls.find(c=>c.command==='save_source').args.draft;assert.equal(draft.action,'update');assert.equal(draft.target_id,'s');
});
test('scan-all is disabled without available sources',async t=>{
 const {d}=await boot(t,{get_state:()=>({sources:[],files:[],jobs:[],busy:false,catalog_path:'test.db'})});assert.equal(d.querySelector('#scanAllButton').disabled,true);
});

test('one-category copying requires explicit acknowledgement and preserves selection on Back',async t=>{
 for(const field of ['matchedPath','unmatchedPath']){
  const {w,d,calls}=await boot(t);d.querySelector('[data-job]').click();d.querySelector('#jobMode').value='copy';d.querySelector('#jobMode').dispatchEvent(new w.Event('change'));d.querySelector('#'+field).value='D:\\selected';
  d.querySelector('#jobForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));await tick();assert.match(d.querySelector('#dialogContent').textContent,/intentionally omitted/);assert.ok(!calls.some(c=>c.command==='start_jobs'));
  d.querySelector('#backCopy').click();assert.equal(d.querySelector('#'+field).value,'D:\\selected');d.querySelector('#jobForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));await tick();d.querySelector('#confirmCopy').click();await tick();
  const request=calls.find(c=>c.command==='start_jobs').args.request;assert.equal(request.confirmed_single_category,true);assert.equal(request[field==='matchedPath'?'unmatched':'matched'],'');
 }
});
test('bulk copy includes available active sources and uses separate namespaces',async t=>{
 const online={...source,id:'s2',name:'Second drive'},offline={...source,id:'offline',online:false};
 const {w,d,calls}=await boot(t,{get_state:()=>({sources:[source,online,offline],files:[],jobs:[],busy:false,catalog_path:'test.db'})});d.querySelector('#copyAllButton').click();assert.match(d.querySelector('#dialogContent').textContent,/1 locations offline/);assert.ok(!calls.some(c=>c.command==='start_jobs'));d.querySelector('#continueAvailable').click();assert.equal(d.querySelector('#jobMode').value,'copy');d.querySelector('#matchedPath').value='D:\\matches';d.querySelector('#jobForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));await tick();assert.match(d.querySelector('#dialogContent').textContent,/name-and-ID/);d.querySelector('#confirmCopy').click();await tick();
 const request=calls.find(c=>c.command==='start_jobs').args.request;assert.deepEqual([...request.source_ids],['s','s2']);assert.equal(request.bulk,true);
});
test('cached thumbnail clicks open the catalog original and settings use native commands',async t=>{
 const photo={id:1,source_id:'s',source_name:'Birds',path:'1DX_1.jpg',name:'1DX_1.jpg',kind:'edited',size:100,present:true,date:'2026-10-17',metadata:'{}',matched:false,preview:true};
 const {d,calls}=await boot(t,{get_state:()=>({sources:[source],files:[photo],jobs:[],busy:false,catalog_path:'test.db',previews_enabled:true,preview_bytes:100}),get_previews:()=>({'1':{data:'ZmFrZQ==',origin:'Original image'}}),open_photo:()=>null,set_previews:()=>null});await tick();assert.ok(d.querySelector('.photo-preview img'));d.querySelector('.photo-preview').click();await tick();assert.equal(calls.find(c=>c.command==='open_photo').args.id,1);
 d.querySelector('[data-command="previewSettings"]').click();assert.match(d.querySelector('#dialogContent').textContent,/160-pixel/);d.querySelector('#storePreviews').checked=false;d.querySelector('#savePreviews').click();await tick();assert.equal(calls.find(c=>c.command==='set_previews').args.enabled,false);
});

test('unchanged polling retains thumbnail DOM and avoids fetching cached data',async t=>{
 const photo={id:1,source_id:'s',source_name:'Birds',path:'a.png',name:'a.png',kind:'edited',size:10,mtime:'1000000000',present:true,metadata:'{"Make":"Canon","Model":"R5","PhotographicSensitivity":"400"}',preview:true,preview_revision:'revision1'};
 const snapshot={sources:[source,{...source,id:'offline',online:false}],files:[photo],jobs:[],busy:false,catalog_path:'test.db',previews_enabled:true,inventory_columns:'["camera","iso"]'};
 const {w,d,calls}=await boot(t,{get_state:()=>snapshot,get_previews:()=>({'1':{data:'ZmFrZQ==',origin:'Original'}}),reveal_photo:()=>null});await tick();const img=d.querySelector('.photo-preview img');assert.ok(img);await w.refresh();assert.equal(d.querySelector('.photo-preview img'),img);assert.equal(calls.filter(c=>c.command==='get_previews').length,1);assert.match(d.querySelector('#files').textContent,/Canon R5/);assert.match(d.querySelector('#files').textContent,/400/);
 assert.match(d.querySelector('#sourceCounts').textContent,/2 registered locations.*1 online.*1 offline/);d.querySelector('#hideOffline').checked=true;d.querySelector('#hideOffline').dispatchEvent(new w.Event('change'));assert.equal(d.querySelectorAll('[data-job]').length,1);d.querySelector('[data-reveal]').click();await tick();assert.equal(calls.find(c=>c.command==='reveal_photo').args.id,1);
 d.querySelector('#scanAllButton').click();d.querySelector('#cancelAvailable').click();assert.ok(!calls.some(c=>c.command==='start_jobs'));
});

test('customized EXIF column choices are saved to the catalog and backup receives the theme',async t=>{
 let columns='[]';const {w,d,calls}=await boot(t,{get_state:()=>({sources:[source],files:[],jobs:[],busy:false,catalog_path:'test.db',inventory_columns:columns}),save_inventory_columns:args=>{columns=JSON.stringify(args.columns)},backup_catalog:()=>true});
 d.querySelector('[data-command="customizeColumns"]').click();d.querySelector('[name=column][value=iso]').checked=true;d.querySelector('[name=column][value=camera]').checked=true;d.querySelector('#columnForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));await tick();assert.equal(columns,'["camera","iso"]');assert.equal(d.querySelectorAll('[data-extra-heading]').length,2);
 d.querySelector('#themePicker').value='soldiers';d.querySelector('[data-command="backupCatalog"]').click();await tick();assert.equal(calls.find(c=>c.command==='backup_catalog').args.theme,'soldiers');assert.match(d.querySelector('#notice').textContent,/backup saved/);
});
