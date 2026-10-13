// The configured Tauri resource is workspace-relative even when Cargo's cache is shared.
import {copyFileSync,mkdirSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const target=process.env.CARGO_TARGET_DIR ? resolve(root,process.env.CARGO_TARGET_DIR) : join(root,'target');
if(target!==join(root,'target')) {
 const name=process.platform==='win32'?'coucou-hook.exe':'coucou-hook';
 const destination=join(root,'target','release');
 mkdirSync(destination,{recursive:true});
 copyFileSync(join(target,'release',name),join(destination,name));
}
