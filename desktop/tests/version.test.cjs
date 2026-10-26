const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawnSync}=require('node:child_process');
test('authoritative identity detects drift and synchronizes every existing version surface',t=>{
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'sarge-identity-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 for(const dir of ['scripts','ui','src-tauri'])fs.mkdirSync(path.join(root,dir));
 for(const file of ['scripts/version.mjs','release.json','package.json','ui/build-info.js','src-tauri/Cargo.toml','src-tauri/Cargo.lock','src-tauri/tauri.conf.json'])fs.copyFileSync(path.join(__dirname,'..',file),path.join(root,file));
 const run=(...args)=>spawnSync(process.execPath,[path.join(root,'scripts/version.mjs'),...args],{encoding:'utf8'});
 const baseline=JSON.parse(fs.readFileSync(path.join(root,'release.json'),'utf8')).version;
 assert.equal(run('--check').status,0);
 for(const file of ['package.json','src-tauri/Cargo.toml','src-tauri/Cargo.lock','src-tauri/tauri.conf.json','ui/build-info.js']){
  const target=path.join(root,file);fs.writeFileSync(target,fs.readFileSync(target,'utf8').replace(baseline,'99.99.99-beta.999'));
  assert.notEqual(run('--check').status,0,file);assert.equal(run().status,0);assert.equal(run('--check').status,0);
 }
 const identity=JSON.parse(fs.readFileSync(path.join(root,'release.json'),'utf8'));
 fs.writeFileSync(path.join(root,'release.json'),JSON.stringify({...identity,version:'99.99.99-beta.8',build:1}));
 assert.equal(run().status,0);assert.equal(run('--check').status,0);assert.match(fs.readFileSync(path.join(root,'ui/build-info.js'),'utf8'),/99\.99\.99-beta\.8/);
 assert.match(run('--release-note').stdout,/<!-- sarge-release: .*"version":"99.99.99-beta.8","build":1.* -->/);
 fs.writeFileSync(path.join(root,'release.json'),JSON.stringify({...identity,build:0}));assert.notEqual(run().status,0);
});
