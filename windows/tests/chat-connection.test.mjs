import {test} from 'node:test';
import assert from 'node:assert/strict';
import {installFakeDom} from './fakedom.mjs';
import {calls,sent} from './tauri.mjs';
installFakeDom();
const {buildChatConnectionSection}=await import('../src/settings/chat-connection.ts');
const {DEFAULT_SETTINGS}=await import('../src/core/state.ts');
const flush=()=>new Promise(r=>setTimeout(r,0));
function fixture(){let config={...DEFAULT_SETTINGS,chatModels:{},chatProvider:'anthropic',screen:'at:1920,0',phoneLink:true};const view=buildChatConnectionSection(()=>config,next=>{config=next;});return {view,get config(){return config}};}
test('configuration renders without querying models or rewriting preferences',()=>{calls.length=0;const {view}=fixture();assert.equal(view.find('select').length,2);assert.deepEqual(sent('chat_models'),[]);assert.deepEqual(sent('save_settings'),[]);});
test('provider/model edits remain drafts until explicit save and preserve unrelated preferences',async()=>{calls.length=0;const f=fixture();const provider=f.view.find('select')[0];const model=f.view.find('input')[0];provider.value='google';provider.fire('change');model.value='configured-model';assert.equal(f.config.chatProvider,'anthropic');assert.deepEqual(sent('save_settings'),[]);f.view.find('button').find(b=>b.textContent==='Salvar conexão do chat').fire('click');await flush();assert.equal(f.config.chatProvider,'google');assert.equal(f.config.chatModels.google,'configured-model');assert.equal(f.config.screen,'at:1920,0');assert.equal(f.config.phoneLink,true);assert.equal(sent('save_settings').length,1);});
test('invalid model is refused before settings IPC',async()=>{calls.length=0;const f=fixture();f.view.find('input')[0].value='bad\0model';f.view.find('button').find(b=>b.textContent==='Salvar conexão do chat').fire('click');await flush();assert.deepEqual(sent('save_settings'),[]);assert.match(f.view.textContent,/modelo válido/);});
