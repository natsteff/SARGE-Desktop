// App update notifications only. All network access and URL validation live in Rust.
let updateStatus=null, updateToken=0;
const buildLabel=id=>id?`${id.version} (Build ${id.build??'unknown'})${id.development?' · development build':''}`:'Identity unavailable';
const fallbackIdentity=window.SARGE_BUILD?{...window.SARGE_BUILD,development:true,source_revision:'unknown',source_modified:null}:null;
$('#appVersion').textContent=buildLabel(fallbackIdentity);
function updateDialog(html){$('#updateDialogContent').innerHTML=html;if(!$('#updateDialog').open)$('#updateDialog').showModal()}
function renderUpdateStatus(){
 if(!updateStatus)return;
 $('#appVersion').textContent=buildLabel(updateStatus.identity);
 $('#updateIdentity').textContent='Installed: '+buildLabel(updateStatus.identity);
 $('#automaticUpdates').checked=updateStatus.automatic_enabled;$('#automaticUpdates').disabled=false;
 const date=value=>value?new Date(value*1000).toLocaleString():'Never';
 $('#updateCheckTimes').textContent=`Last attempt: ${date(updateStatus.last_attempt)}. Last successful check: ${date(updateStatus.last_success)}.${updateStatus.backoff_until>Date.now()/1000?' GitHub cooldown until '+date(updateStatus.backoff_until)+'.':''}`;
}
async function readUpdateStatus(){updateStatus=await invoke('get_update_status');renderUpdateStatus();return updateStatus}
function showAbout(){
 updateToken++;
 const id=updateStatus?.identity||fallbackIdentity;
 updateDialog(`<h2>About SARGE Desktop</h2><p>${esc(buildLabel(id))}</p><dl class="photo-details"><dt>Source revision</dt><dd>${esc(id?.source_revision||'unknown')}</dd><dt>Uncommitted source changes</dt><dd>${id?.source_modified===true?'Yes':id?.source_modified===false?'No':'Unknown'}</dd><dt>Platform</dt><dd>${esc(id?.platform||'unknown')}</dd></dl><p>Source: github.com/natsteff/SARGE-Desktop · AGPL-3.0-only</p><p>Your original media stays in place. Optional release checks contact GitHub; no catalog contents, filenames or paths are sent.</p><button id="aboutCheckUpdates">Check for Updates…</button>`);
 $('#aboutCheckUpdates').onclick=manualUpdateCheck;
}
async function viewUpdateRelease(url,errorTarget){try{await invoke('open_update_release',{url})}catch(e){errorTarget.textContent=String(e)}}
function showUpdateResult(result){
 const title={newer:'Update available',current:'Version and build confirmed',unknown:'Update status unknown',source_only:'No suitable installer',failure:'Update check failed'}[result.status]||'Update status unknown';
 updateDialog(`<h2>${title}</h2><p>Installed: ${esc(buildLabel(result.installed))}</p>${result.release?`<p>Release: ${esc(buildLabel(result.release))}</p>`:''}<p>${esc(result.message)}</p>${result.release?'<button id="viewUpdateRelease" class="primary">View Release</button> ':''}<button id="retryUpdateCheck">Check again</button><p id="updateResultError" role="alert"></p>`);
 if(result.release)$('#viewUpdateRelease').onclick=()=>viewUpdateRelease(result.release.url,$('#updateResultError'));
 $('#retryUpdateCheck').onclick=manualUpdateCheck;
}
async function manualUpdateCheck(){
 const token=++updateToken;
 updateDialog(`<h2>Checking for Updates…</h2><p>Installed: ${esc(buildLabel(updateStatus?.identity||fallbackIdentity))}</p><p>Contacting official SARGE Desktop GitHub Releases. You can close this window and continue using SARGE.</p>`);
 try{
  const result=await invoke('check_for_updates',{manual:true});
  if(token!==updateToken||!$('#updateDialog').open)return;
  showUpdateResult(result);
 }catch(e){if(token===updateToken&&$('#updateDialog').open)showUpdateResult({status:'failure',installed:updateStatus?.identity||fallbackIdentity,message:String(e)})}
 try{await readUpdateStatus()}catch(e){} // Status refresh cannot replace a result or affect app jobs.
}
function automaticUpdateNotice(result){
 if(result.status!=='newer'||!result.release||!updateStatus?.automatic_enabled)return;
 const area=$('#updateNotice');area.hidden=false;
 area.innerHTML=`<div><b>SARGE ${esc(buildLabel(result.release))} is available.</b><p>Open the official release page to review it. Downloading and installing is your choice.</p></div><div class="update-actions"><button id="automaticViewRelease">View Release</button><button id="laterUpdate">Later</button></div><span id="automaticUpdateError" role="alert"></span>`;
 $('#automaticViewRelease').onclick=()=>viewUpdateRelease(result.release.url,$('#automaticUpdateError'));
 $('#laterUpdate').onclick=async()=>{const button=$('#laterUpdate');button.disabled=true;try{await invoke('dismiss_update',{key:result.release.key});area.hidden=true;area.replaceChildren()}catch(e){$('#automaticUpdateError').textContent=String(e);button.disabled=false}};
}
async function initializeUpdates(){
 try{
  const status=await readUpdateStatus();$('#updatePreferenceStatus').textContent='Preference changes save immediately on this computer.';
  if(!status.automatic_enabled)return; // No automatic check command, and no automatic HTTP request.
  const result=await invoke('check_for_updates',{manual:false});
  await readUpdateStatus(); // Re-read the toggle in case it was disabled during the request.
  automaticUpdateNotice(result);
 }catch(e){/* Offline/automatic failures remain quiet. Manual checking is always available. */}
}
$('#automaticUpdates').onchange=async e=>{
 const input=e.target,enabled=input.checked;input.disabled=true;
 try{await invoke('set_update_automatic',{enabled});await readUpdateStatus();$('#updatePreferenceStatus').textContent=enabled?'Saved. Automatic checks will run on future startups when due.':'Saved. Automatic network checks are off.';if(!enabled){$('#updateNotice').hidden=true;$('#updateNotice').replaceChildren()}}
 catch(e){input.checked=updateStatus?.automatic_enabled??false;$('#updatePreferenceStatus').textContent=String(e)}finally{input.disabled=false}
};
$('#manualUpdateCheck').onclick=manualUpdateCheck;
$('#closeUpdateDialog').onclick=()=>$('#updateDialog').close();
$('#updateDialog').addEventListener('close',()=>{updateToken++});
$('#updateDialog').addEventListener('cancel',()=>{updateToken++});
if(window.__TAURI__?.event){
 window.__TAURI__.event.listen('check-for-updates',manualUpdateCheck).catch(()=>{});
 window.__TAURI__.event.listen('show-about',showAbout).catch(()=>{});
}
setTimeout(initializeUpdates,0); // Never await an update check before rendering/browsing the catalog.
