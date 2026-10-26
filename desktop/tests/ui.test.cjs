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
 const calls=[],events={};const handlers={get_state:()=>({sources:[source],files:[],jobs:[],busy:false,catalog_path:'test/catalog.db'}),source_candidates:()=>[],save_source:()=> 's',start_jobs:()=>({queued:1}),...overrides};
 dom.window.__TAURI__={event:{listen:async(name,callback)=>{events[name]=callback;return ()=>{}}},core:{invoke:async(command,args)=>{calls.push({command,args});if(!handlers[command])throw Error('Unexpected command '+command);return handlers[command](args)}}};
 dom.window.eval(['theme.js','inventory.js','mapping-settings.js','media.js','app.js'].map(name=>fs.readFileSync(path.join(root,name),'utf8')).join('\n'));await tick();
 return {w:dom.window,d:dom.window.document,calls,events};
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
 d.querySelector('[data-settings-section="previews"]').click();assert.match(d.querySelector('#settings-previews').textContent,/160-pixel/);d.querySelector('#storePreviews').checked=false;d.querySelector('#savePreviews').click();await tick();assert.equal(calls.find(c=>c.command==='set_previews').args.enabled,false);
});

test('unchanged polling retains thumbnail DOM and avoids fetching cached data',async t=>{
 const photo={id:1,source_id:'s',source_name:'Birds',path:'a.png',name:'a.png',kind:'edited',size:10,mtime:'1000000000',present:true,metadata:'{"Make":"Canon","Model":"R5","PhotographicSensitivity":"400"}',preview:true,preview_revision:'revision1'};
 const snapshot={sources:[source,{...source,id:'offline',online:false}],files:[photo],jobs:[],busy:false,catalog_path:'test.db',previews_enabled:true,inventory_columns:'["camera","iso"]'};
 const {w,d,calls}=await boot(t,{get_state:()=>snapshot,get_previews:()=>({'1':{data:'ZmFrZQ==',origin:'Original'}}),reveal_photo:()=>null});await tick();const img=d.querySelector('.photo-preview img');assert.ok(img);await w.refresh();assert.equal(d.querySelector('.photo-preview img'),img);assert.equal(calls.filter(c=>c.command==='get_previews').length,1);assert.match(d.querySelector('#files').textContent,/R5/);assert.match(d.querySelector('#files').textContent,/400/);
 assert.match(d.querySelector('#sourceCounts').textContent,/2 registered locations.*1 online.*1 offline/);d.querySelector('#hideOffline').checked=true;d.querySelector('#hideOffline').dispatchEvent(new w.Event('change'));assert.equal(d.querySelectorAll('[data-job]').length,1);d.querySelector('[data-reveal]').click();await tick();assert.equal(calls.find(c=>c.command==='reveal_photo').args.id,1);
 d.querySelector('#scanAllButton').click();d.querySelector('#cancelAvailable').click();assert.ok(!calls.some(c=>c.command==='start_jobs'));
});

test('customized EXIF column choices are saved to the catalog and backup receives the theme',async t=>{
 let columns='[]';const {w,d,calls}=await boot(t,{get_state:()=>({sources:[source],files:[],jobs:[],busy:false,catalog_path:'test.db',inventory_columns:columns}),save_inventory_columns:args=>{columns=JSON.stringify(args.columns)},backup_catalog:()=>true});
 d.querySelector('[data-command="customizeColumns"]').click();d.querySelector('[name=column][value=iso]').checked=true;d.querySelector('[name=column][value=camera]').checked=true;d.querySelector('#columnForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));await tick();assert.equal(columns,'["camera","iso"]');assert.equal(d.querySelectorAll('[data-extra-heading]').length,2);
 d.querySelector('#themePicker').value='soldiers';d.querySelector('[data-command="backupCatalog"]').click();await tick();assert.equal(calls.find(c=>c.command==='backup_catalog').args.theme,'soldiers');assert.match(d.querySelector('#notice').textContent,/backup saved/);
});

function photo(id,metadata={},mapped={}){return {id,source_id:'s',source_name:'Birds',path:`P_${id}.cr2`,name:`P_${id}.cr2`,kind:'raw',size:1000*id,mtime:'1000000000',present:true,date:'2026-10-17',metadata:JSON.stringify(metadata),mapped:JSON.stringify(mapped),matched:id===1}}
const rowIds=d=>[...d.querySelectorAll('[data-detail]')].map(b=>Number(b.dataset.detail));
test('every selected EXIF/mapped column sorts numerically or by date with unknown values last',async t=>{
 const files=[photo(1,{PhotographicSensitivity:'800',ExposureTime:'1/1.6666666666666667 s',DateTimeOriginal:'2020:01:02 12:00:00',PixelXDimension:'6000 pixels',PixelYDimension:'4000 pixels'},{camera_body:{value:'Z'}}),photo(2,{PhotographicSensitivity:'100',ExposureTime:'1/60 s',DateTimeOriginal:'2019:01:02 12:00:00',PixelXDimension:'5000 pixels',PixelYDimension:'4000 pixels'},{camera_body:{value:'A'}}),photo(3)];
 const {d,w}=await boot(t,{get_state:()=>({sources:[source],files,jobs:[],inventory_columns:'["iso","shutter","captured","dimensions","mapped_camera_body"]'})});
 for(let key of ['iso','shutter','captured','dimensions','mapped_camera_body']){let b=d.querySelector(`[data-sort="${key}"]`);assert.ok(b);b.click();assert.deepEqual(rowIds(d),[2,1,3],key);d.querySelector(`[data-sort="${key}"]`).click();assert.deepEqual(rowIds(d),[1,2,3],key+' reverse');}
 assert.equal(w.shutterText(0.6),'0.6 s');assert.equal(w.shutterText(1/60),'1/60 s');assert.equal(w.shutterText(1/250),'1/250 s');assert.equal(w.shutterText(30),'30 s');assert.match(d.querySelector('#files').textContent,/0.6 s/);
});
test('EXIF dialog combines multiple choices, numeric ranges, missing metadata and removable filters offline',async t=>{
 const files=[photo(1,{Model:'"R5"',PhotographicSensitivity:'100'}),photo(2,{Model:'"R6"',PhotographicSensitivity:'800'}),photo(3)];
 const {d,w,calls}=await boot(t,{get_state:()=>({sources:[{...source,online:false}],files,jobs:[],inventory_columns:'[]'})});
 d.querySelector('#exifFilterButton').click();for(let value of ['R5','R6']){let input=[...d.querySelectorAll('#facetValues input')].find(b=>b.value==='value:'+value);input.checked=true;input.dispatchEvent(new w.Event('change',{bubbles:true}))}
 d.querySelector('#filterField').value='iso';d.querySelector('#filterField').dispatchEvent(new w.Event('change'));d.querySelector('#rangeMin').value='200';d.querySelector('#rangeMin').dispatchEvent(new w.Event('input'));d.querySelector('#applyExifFilters').click();assert.deepEqual(rowIds(d),[2]);assert.equal(d.querySelectorAll('[data-clear-filter]').length,2);assert.equal(d.querySelectorAll('[data-extra-heading]').length,0);
 d.querySelector('[data-clear-filter="iso"]').click();assert.deepEqual(rowIds(d),[1,2]);d.querySelector('#clearExifFilters').click();assert.equal(rowIds(d).length,3);
 d.querySelector('#exifFilterButton').click();let missing=d.querySelector('#facetValues input[value="missing"]');missing.checked=true;missing.dispatchEvent(new w.Event('change',{bubbles:true}));d.querySelector('#applyExifFilters').click();assert.deepEqual(rowIds(d),[3]);assert.ok(!calls.some(c=>['pick_folder','open_photo','get_previews'].includes(c.command)));
});
test('mapped fields are paired beside actual EXIF and filter separately',async t=>{
 const files=[photo(1,{Model:'Canon EOS 6D'},{camera_body:{value:'1DX',rule_id:'default-camera-cr2'}}),photo(2,{Model:'Canon EOS 6D'},{camera_body:{value:'6D',rule_id:'default-camera-cr2'}})];
 const {d,w}=await boot(t,{get_state:()=>({sources:[source],files,jobs:[],inventory_columns:'["camera","iso","mapped_camera_body"]'})});
 assert.deepEqual([...d.querySelectorAll('[data-extra-heading]')].map(h=>h.dataset.extraHeading),['camera','mapped_camera_body','iso']);assert.match(d.querySelector('#files').textContent,/Canon EOS 6D/);assert.match(d.querySelector('#files').textContent,/1DX/);
 d.querySelector('#exifFilterButton').click();d.querySelector('#filterField').value='mapped_camera_body';d.querySelector('#filterField').dispatchEvent(new w.Event('change'));let selected=d.querySelector('input[value="value:1DX"]');selected.checked=true;selected.dispatchEvent(new w.Event('change',{bubbles:true}));d.querySelector('#applyExifFilters').click();assert.deepEqual(rowIds(d),[1]);
});
test('location summaries retain category counts and bytes for offline sources',async t=>{
 const a=photo(1),b={...photo(2),kind:'edited'},c=photo(3);const {d}=await boot(t,{get_state:()=>({sources:[{...source,online:false}],files:[a,b,c],jobs:[]})});
 const cards=[...d.querySelectorAll('.source-summary>div')];assert.equal(cards.length,6);assert.match(cards[0].textContent,/All files3.*6.00 KB/);assert.match(cards[1].textContent,/RAW originals2.*4.00 KB/);assert.match(cards[2].textContent,/Edited \/ exported1.*2.00 KB/);assert.match(cards[3].textContent,/Matched RAWs1.*1.00 KB/);assert.match(cards[4].textContent,/Unmatched RAWs1.*3.00 KB/);
});
test('bulk activity groups by explicit batch identity and preserves expanded results during polling',async t=>{
 const base={mode:'scan',started:'2026-10-18T12:00:00Z',status:'complete',count:2,copied:0,identical:0,skipped:0,errors:'[]',batch_bulk:true};
 const jobs=[{...base,id:'a',name:'A',batch_id:'batch-one'},{...base,id:'b',name:'B',batch_id:'batch-one',count:3,status:'failed',errors:'["Disconnected"]'},{...base,id:'c',name:'C',batch_id:'batch-two'},{...base,id:'legacy',name:'Old job',batch_id:'',batch_bulk:false}];
 const {d,w}=await boot(t,{get_state:()=>({sources:[source],files:[],jobs})});assert.equal(d.querySelectorAll('.activity-batch').length,3);let block=d.querySelector('.activity-batch');assert.match(block.textContent,/2 locations/);assert.match(block.textContent,/5 indexed/);assert.match(block.textContent,/partial/);assert.match(block.textContent,/Disconnected/);d.querySelector('[data-batch="batch-one"]').open=true;await w.refresh();assert.equal(d.querySelector('[data-batch="batch-one"]').open,true);
});
test('visual mapping builder highlights a value, previews scope, and saves default rule changes',async t=>{
 const rules=[{id:'default-camera-cr2',field:'camera_body',before:'',after:'_',extensions:['cr2'],enabled:true}],f={...photo(1),name:'1DX_1234.cr2'};let saved=rules;
 const {d,w,calls}=await boot(t,{get_state:()=>({sources:[{...source,online:false}],files:[f],jobs:[],filename_rules:saved}),test_filename_rules:()=>[{name:f.name,mapped:{camera_body:{value:'1DX'}}}],save_filename_rules:({rules})=>{saved=rules;return 1}});
 d.querySelector('[data-settings-section="naming"]').click();assert.match(d.querySelector('#settings-naming').textContent,/Camera body \(Mapped\).*CR2/);d.querySelector('[data-edit-rule="0"]').click();let input=d.querySelector('#namingExample');input.setSelectionRange(0,3);input.dispatchEvent(new w.Event('select'));d.querySelector('#useNamingSelection').click();d.querySelector('#namingAllTypes').checked=true;d.querySelector('#namingAllTypes').dispatchEvent(new w.Event('change'));d.querySelector('#testNamingRule').click();await tick();assert.match(d.querySelector('#namingPreview').textContent,/1DX_1234.cr2.*1DX/);d.querySelector('#useNamingRule').click();d.querySelector('#saveNamingRules').click();await tick();let rule=calls.find(c=>c.command==='save_filename_rules').args.rules[0];assert.equal(rule.field,'camera_body');assert.equal(rule.before,'');assert.equal(rule.after,'_');assert.deepEqual([...rule.extensions],['cr2','jpg','jpeg','png']);assert.match(d.querySelector('#notice').textContent,/offline locations/);
});
test('large facet lists are bounded and retain selections across searches',async t=>{
 const files=Array.from({length:220},(_,i)=>photo(i+1,{Model:'Camera '+i}));const {d,w}=await boot(t,{get_state:()=>({sources:[source],files,jobs:[]})});d.querySelector('#exifFilterButton').click();assert.equal(d.querySelectorAll('#facetValues input').length,150);let first=d.querySelector('#facetValues input');first.checked=true;first.dispatchEvent(new w.Event('change',{bubbles:true}));d.querySelector('#facetSearch').value='Camera 219';d.querySelector('#facetSearch').dispatchEvent(new w.Event('input'));assert.equal(d.querySelectorAll('#facetValues input').length,1);let last=d.querySelector('#facetValues input');last.checked=true;last.dispatchEvent(new w.Event('change',{bubbles:true}));d.querySelector('#applyExifFilters').click();assert.equal(rowIds(d).length,2);
});
test('date ranges, numeric EXIF precision and invalid ranges are handled explicitly',async t=>{
 const files=[photo(1,{DateTimeOriginal:'2020:01:02 23:59:59',ExposureTime:'legacy display',SargeNumeric:{ExposureTime:0.6}}),photo(2,{DateTimeOriginal:'2020:01:03 00:00:00'})];const {d,w}=await boot(t,{get_state:()=>({sources:[source],files,jobs:[],inventory_columns:'["shutter"]'})});assert.match(d.querySelector('#files').textContent,/0.6 s/);d.querySelector('#exifFilterButton').click();d.querySelector('#filterField').value='captured';d.querySelector('#filterField').dispatchEvent(new w.Event('change'));d.querySelector('#rangeMin').value='2020-01-03';d.querySelector('#rangeMax').value='2020-01-02';d.querySelector('#applyExifFilters').click();assert.match(d.querySelector('#filterError').textContent,/Minimum/);d.querySelector('#rangeMin').value='2020-01-02';d.querySelector('#applyExifFilters').click();assert.deepEqual(rowIds(d),[1]);
});
function videoFile(id=1){return {...photo(id,{DurationSeconds:125.5,ImageWidth:1920,ImageLength:1080}),name:`clip${id}.mp4`,path:`clips/clip${id}.mp4`,kind:'video',matched:false,matchable:false}}
test('video-only catalogs show totals, metadata and independent open actions without thumbnails or matches',async t=>{
 const f=videoFile(),snapshot={sources:[source],files:[f],jobs:[],inventory_columns:'["duration","dimensions"]'};const {d,w,calls}=await boot(t,{get_state:()=>snapshot,open_photo:()=>null,reveal_photo:()=>null});
 assert.equal(d.querySelector('#heading').textContent,'Media Inventory');assert.equal(d.querySelector('#kind').value,'');assert.equal(d.querySelectorAll('#metrics .metric').length,5);assert.match(d.querySelector('#metrics .video-focus').textContent,/Videos1/);assert.match(d.querySelector('#files').textContent,/Not applicable/);assert.match(d.querySelector('#files').textContent,/2:05.5/);assert.match(d.querySelector('#files').textContent,/1920 × 1080/);assert.equal(d.querySelectorAll('.photo-preview img').length,0);
 d.querySelector('.video-icon').click();d.querySelector('[data-reveal]').click();await tick();assert.ok(calls.some(c=>c.command==='open_photo'));assert.ok(calls.some(c=>c.command==='reveal_photo'));d.querySelector('[data-detail]').click();assert.match(d.querySelector('#dialogContent').textContent,/Video · Matching not applicable/);assert.equal(d.querySelector('#manual'),null);d.querySelector('#dialog').close();
 d.querySelector('#status').value='unmatched';d.querySelector('#status').dispatchEvent(new w.Event('change'));assert.equal(rowIds(d).length,0);d.querySelector('#status').value='';d.querySelector('#status').dispatchEvent(new w.Event('change'));assert.equal(rowIds(d).length,1);
 snapshot.sources=[{...source,online:false}];await w.refresh();assert.equal(d.querySelector('.video-icon').disabled,true);assert.equal(d.querySelector('[data-reveal]').disabled,true);assert.match(d.querySelector('#files').textContent,/2:05.5/);
});
test('all-media filtering distinguishes images, RAW, videos, and non-matching image formats',async t=>{
 const files=[photo(1),{...photo(2),kind:'edited',name:'sample.bmp',matchable:false},videoFile(3)];const {d,w}=await boot(t,{get_state:()=>({sources:[source],files,jobs:[]})});assert.equal(rowIds(d).length,3);
 for(let [kind,expected] of [['images',2],['video',1],['raw',1],['edited',1]]){d.querySelector('#kind').value=kind;d.querySelector('#kind').dispatchEvent(new w.Event('change'));assert.equal(rowIds(d).length,expected)}d.querySelector('#kind').value='';d.querySelector('#status').value='unconfigured';d.querySelector('#status').dispatchEvent(new w.Event('change'));assert.deepEqual(rowIds(d),[2]);assert.match(d.querySelector('#files').textContent,/Not configured/);
});
test('include-videos option requires an unmatched destination and is explicit in copy confirmation',async t=>{
 const {d,w,calls}=await boot(t);d.querySelector('[data-job]').click();d.querySelector('#jobMode').value='copy';d.querySelector('#jobMode').dispatchEvent(new w.Event('change'));assert.equal(d.querySelector('#includeVideos').disabled,true);assert.equal(d.querySelector('#includeVideos').checked,false);d.querySelector('#unmatchedPath').value='D:\\unmatched';w.updateVideoCopyOption();assert.equal(d.querySelector('#includeVideos').disabled,false);d.querySelector('#includeVideos').checked=true;d.querySelector('#jobForm').dispatchEvent(new w.Event('submit',{bubbles:true,cancelable:true}));assert.match(d.querySelector('#dialogContent').textContent,/Videos WILL be included/);d.querySelector('#confirmCopy').click();await tick();assert.equal(calls.find(c=>c.command==='start_jobs').args.request.include_videos,true);
});
test('Catalog tools copy videos independently of photo matches with explicit review',async t=>{
 const {d,w,calls}=await boot(t,{get_state:()=>({sources:[source],files:[videoFile()],jobs:[]})});d.querySelector('[data-settings-section="catalog"]').click();d.querySelector('[data-command="copyVideos"]').click();d.querySelector('#videoDestination').value='D:\\videos';d.querySelector('#reviewVideoCopy').click();assert.match(d.querySelector('#dialogContent').textContent,/1 inventoried videos/);assert.ok(!calls.some(c=>c.command==='start_jobs'));d.querySelector('#confirmVideoCopy').click();await tick();const req=calls.find(c=>c.command==='start_jobs').args.request;assert.equal(req.mode,'copy-videos');assert.equal(req.matched,'');assert.equal(req.confirmed_video_copy,true);
});
test('format settings retain CR2 JPG JPEG PNG defaults and allow another RAW pair',async t=>{
 const {d,w,calls}=await boot(t,{save_media_formats:()=>null});d.querySelector('[data-settings-section="formats"]').click();assert.deepEqual([...d.querySelectorAll('[data-match-target]:checked')].map(e=>e.dataset.matchTarget),['jpg','jpeg','png']);d.querySelector('#matchingRaw').value='nef';d.querySelector('#matchingRaw').dispatchEvent(new w.Event('change'));let jpg=d.querySelector('[data-match-target="jpg"]');jpg.checked=true;jpg.dispatchEvent(new w.Event('change'));d.querySelector('#saveMediaFormats').click();await tick();const settings=calls.find(c=>c.command==='save_media_formats').args.settings;assert.ok(settings.rules.some(r=>r.raw==='cr2'&&r.targets.includes('png')));assert.ok(settings.rules.some(r=>r.raw==='nef'&&r.targets.includes('jpg')));assert.ok(settings.enabled.includes('mp4'));
});

test('job notice follows live busy state and clears on completion',async t=>{
 let busy=false;const {w,d}=await boot(t,{start_jobs:()=>{busy=true;return {queued:1}},get_state:()=>({sources:[source],files:[],jobs:[],busy,catalog_path:'test.db'})});
 d.querySelector('#scanAllButton').click();await tick();assert.match(d.querySelector('#notice').textContent,/Results update below/);
 d.querySelector('[data-view="drives"]').click();assert.match(d.querySelector('#notice').textContent,/See Scan activity/);
 busy=false;await w.eval('refresh()');assert.equal(d.querySelector('#notice').textContent,'');
});
test('open source uses native action and is disabled offline',async t=>{
 const {d,calls}=await boot(t,{open_source:()=>null,get_state:()=>({sources:[source,{...source,id:'offline',online:false}],files:[],jobs:[],busy:false,catalog_path:'test.db'})});
 const buttons=d.querySelectorAll('[data-open-source]');assert.equal(buttons[0].disabled,false);assert.equal(buttons[1].disabled,true);buttons[0].click();await tick();assert.equal(calls.find(c=>c.command==='open_source').args.id,'s');
});

test('matching settings show multiple pairs and configured formats first',async t=>{
 const {w,d,calls}=await boot(t,{save_media_formats:()=>null});w.eval('formatSettings()');
 assert.match(d.querySelector('#configuredPairs').textContent,/CR2/);assert.match(d.querySelector('#matchingRaw option').textContent,/CR2 — configured/);
 d.querySelector('#matchingRaw').value='cr3';d.querySelector('#matchingRaw').dispatchEvent(new w.Event('change'));
 const target=d.querySelector('[data-match-target="jpg"]');target.checked=true;target.dispatchEvent(new w.Event('change'));
 assert.match(d.querySelector('#configuredPairs').textContent,/CR2.*CR3/);assert.deepEqual([...d.querySelector('#matchingRaw').options].slice(0,2).map(o=>o.value),['cr2','cr3']);
 d.querySelector('#saveMediaFormats').click();await tick();assert.equal(calls.find(c=>c.command==='save_media_formats').args.settings.rules.length,2);
});
test('default filename rule can be removed and restored with clear button state',async t=>{
 const {w,d}=await boot(t);w.eval('namingSettings()');assert.equal(d.querySelector('#restoreNamingDefault').disabled,true);
 d.querySelector('[data-delete-rule]').click();assert.equal(d.querySelector('#restoreNamingDefault').disabled,false);
 d.querySelector('#restoreNamingDefault').click();assert.equal(d.querySelector('#restoreNamingDefault').disabled,true);assert.ok(d.querySelector('[data-delete-rule]'));
});

test('gear and native menu share settings, retain drafts across navigation and preserve inventory filters',async t=>{
 const {w,d,calls,events}=await boot(t);
 assert.equal(d.querySelectorAll('aside [data-command]').length,1);
 d.querySelector('#search').value='Birds';d.querySelector('#settingsGear').click();
 assert.equal(d.querySelector('#heading').textContent,'Settings');assert.equal(d.querySelector('#metrics').hidden,true);
 assert.equal(d.querySelector('#settings-appearance').hidden,false);
 d.querySelector('[data-settings-section="formats"]').click();
 d.querySelector('#matchingRaw').value='nef';d.querySelector('#matchingRaw').dispatchEvent(new w.Event('change'));
 let target=d.querySelector('[data-match-target="jpg"]');target.checked=true;target.dispatchEvent(new w.Event('change'));
 d.querySelector('[data-settings-section="previews"]').click();d.querySelector('[data-settings-section="formats"]').click();
 assert.equal(d.querySelector('#matchingRaw').value,'nef');assert.equal(d.querySelector('[data-match-target="jpg"]').checked,true);
 assert.ok(!calls.some(c=>c.command==='save_media_formats'));
 d.querySelector('[data-view="inventory"]').click();assert.equal(d.querySelector('#search').value,'Birds');assert.equal(d.querySelector('#metrics').hidden,false);
 events['open-settings']();assert.equal(d.querySelector('#settings-formats').hidden,false);assert.equal(d.querySelector('#dialog').open,false);
 d.querySelector('[data-settings-section="naming"]').click();d.querySelector('#addNamingRule').click();assert.equal(d.querySelector('#dialog').open,true);
 d.querySelector('#cancelNamingRule').click();assert.equal(d.querySelector('#dialog').open,false);assert.equal(d.querySelector('#settings-naming').hidden,false);
});
test('settings preserve drafts on backend errors and reflect busy job restrictions',async t=>{
 let busy=true;const {w,d}=await boot(t,{get_state:()=>({sources:[source],files:[],jobs:[],busy}),save_media_formats:()=>{throw Error('Test save failure')}});
 d.querySelector('[data-settings-section="formats"]').click();assert.equal(d.querySelector('#saveMediaFormats').disabled,true);
 busy=false;await w.refresh();assert.equal(d.querySelector('#saveMediaFormats').disabled,false);
 d.querySelector('#saveMediaFormats').click();await tick();assert.match(d.querySelector('#formatsError').textContent,/Test save failure/);assert.ok(d.querySelector('#matchingRaw'));
 d.querySelector('[data-settings-section="previews"]').click();busy=true;await w.refresh();assert.equal(d.querySelector('#savePreviews').disabled,true);assert.equal(d.querySelector('#generatePreviews').disabled,true);
});

test('Drives video-only action skips offline sources only after confirmation and reviews before copying',async t=>{
 const offline={...source,id:'offline',name:'Offline USB',online:false};
 const {d,calls}=await boot(t,{get_state:()=>({sources:[source,offline],files:[videoFile()],jobs:[],busy:false})});
 assert.equal(d.querySelector('[data-command="attachedDrives"]'),null);
 d.querySelector('[data-view="drives"]').click();
 const action=d.querySelector('#copyVideosButton');assert.equal(action.textContent,'Copy Videos ONLY');assert.equal(action.disabled,false);action.click();
 assert.match(d.querySelector('#dialogContent').textContent,/Offline USB/);assert.equal(d.querySelector('#videoDestination'),null);
 d.querySelector('#continueAvailable').click();assert.equal(d.querySelectorAll('[name="videoSource"]').length,1);
 d.querySelector('#videoDestination').value='D:\\videos';d.querySelector('#reviewVideoCopy').click();
 assert.ok(!calls.some(c=>c.command==='start_jobs'));d.querySelector('#confirmVideoCopy').click();await tick();
 const request=calls.find(c=>c.command==='start_jobs').args.request;
 assert.equal(request.mode,'copy-videos');assert.deepEqual([...request.source_ids],['s']);assert.equal(request.confirmed_video_copy,true);
});
test('Drives video-only action is disabled while busy or without online sources',async t=>{
 let snapshot={sources:[source],files:[],jobs:[],busy:true};
 const {w,d}=await boot(t,{get_state:()=>snapshot});assert.equal(d.querySelector('#copyVideosButton').disabled,true);
 snapshot={...snapshot,busy:false,sources:[{...source,online:false}]};await w.refresh();assert.equal(d.querySelector('#copyVideosButton').disabled,true);
 snapshot={...snapshot,sources:[source]};await w.refresh();assert.equal(d.querySelector('#copyVideosButton').disabled,false);
});
