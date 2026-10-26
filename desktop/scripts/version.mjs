import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const identity=JSON.parse(fs.readFileSync(path.join(root,'release.json'),'utf8'));
if(identity.schema!==1||!/^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:alpha|beta|rc)(?:\.(?:0|[1-9]\d*))+)?$/.test(identity.version)||!Number.isSafeInteger(identity.build)||identity.build<1)throw Error('Invalid release.json identity');
if(process.argv.includes('--release-note')){console.log(`<!-- sarge-release: ${JSON.stringify(identity)} -->`);process.exit(0)}
const check=process.argv.includes('--check');
function write(file,text){const target=path.join(root,file);if(check){if(fs.readFileSync(target,'utf8').replace(/\r\n/g,'\n')!==text.replace(/\r\n/g,'\n'))throw Error(`${file} differs from release.json; run pnpm version:sync`)}else fs.writeFileSync(target,text)}
for(const file of ['package.json','src-tauri/tauri.conf.json']){const text=fs.readFileSync(path.join(root,file),'utf8');write(file,text.replace(/("version":\s*")[^"]+"/,`$1${identity.version}"`))}
const cargo=fs.readFileSync(path.join(root,'src-tauri/Cargo.toml'),'utf8');write('src-tauri/Cargo.toml',cargo.replace(/(\[package\][\s\S]*?\nversion = ")[^"]+"/,`$1${identity.version}"`));
const lock=fs.readFileSync(path.join(root,'src-tauri/Cargo.lock'),'utf8');write('src-tauri/Cargo.lock',lock.replace(/(name = "sarge-desktop"\r?\nversion = ")[^"]+"/,`$1${identity.version}"`));
write('ui/build-info.js',`// Generated from desktop/release.json by scripts/version.mjs.\nwindow.SARGE_BUILD = Object.freeze(${JSON.stringify(identity)});\n`);
console.log(check?'Version/build identity verified.':'Version/build metadata synchronized.');
