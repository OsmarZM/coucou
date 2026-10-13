import {test,beforeEach} from 'node:test';
import assert from 'node:assert/strict';
import {installFakeDom} from './fakedom.mjs';
import {internals,calls,sent} from './tauri.mjs';
installFakeDom();
window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
const {State,DEFAULT_SETTINGS}=await import('../src/core/state.ts');
const {AgentChat}=await import('../src/personal/core/agent-chat.ts');
const {buildPrompt}=await import('../src/views/chat.ts');
const invoke=internals.invoke;
internals.invoke=async(cmd,args)=>{
 const result=await invoke(cmd,args);
 if(cmd==='agent_chat_status')return [{agent:'api',available:true,personalSupported:true,readonlyOnly:true,detail:'fixture'}];
 if(cmd==='documents_list')return [];
 if(cmd==='documents_ingest')return [{id:'fixture-pdf',conversationId:args.conversationId,name:'note.pdf',kind:'pdf',status:'queued',size:100,sha256:'fixture',createdAt:0,coverage:null,message:null}];
 return result;
};
const flush=()=>new Promise(r=>setTimeout(r,10));
beforeEach(()=>{calls.length=0;State.settings={...DEFAULT_SETTINGS,chatModels:{},chatProvider:'google'};State.agentRuntimeAvailable=true;State.mode='expanded';State.view='prompt';State.droppedFile=null;});
function open(){const view=buildPrompt(()=>{});view.sync();return view;}
test('personal agent defaults to shared configured provider without installing Hermes or sending a prompt',async()=>{
 const view=open();await flush();assert.equal(AgentChat.provider,'api');assert.equal(view.el.querySelector('.chat-identity'),null);assert.equal(view.el.querySelector('.agent-model-button'),null);assert.deepEqual(sent('agent_chat_start'),[]);assert.deepEqual(sent('managed_service_provision'),[]);assert.equal(State.settings.chatProvider,'google');
});
test('drop reaches personal attachment scope once only after inbox copy, and never sends a task automatically',async()=>{
 const view=open();State.droppedFile={name:'note.pdf',path:'D:/Documents/note.pdf'};view.sync();await flush();assert.deepEqual(sent('documents_ingest'),[]);
 State.droppedFile={name:'note.pdf',path:'D:/Coucou/inbox/note.pdf'};view.sync();await flush();view.sync();await flush();assert.equal(sent('documents_ingest').length,1);assert.deepEqual(sent('documents_ingest')[0].paths,['D:/Coucou/inbox/note.pdf']);assert.equal(sent('documents_ingest')[0].conversationId,'personal-main');assert.deepEqual(sent('agent_chat_start'),[]);
});

test('one unified chat has no mode menu and only one message editor',()=>{const view=open();assert.equal(view.el.querySelector('.chat-modes'),null);assert.equal(view.el.find('.chat-input').length,1);assert.equal(view.el.find('.tab').length,0);});

test('automatic chat exposes no engine, provider or model controls and preserves saved configuration',()=>{const view=open();assert.equal(view.el.querySelector('.agent-provider-controls'),null);assert.equal(view.el.querySelector('.picker-chips'),null);assert.equal(State.settings.chatProvider,'google');assert.deepEqual(sent('chat_models'),[]);assert.deepEqual(sent('save_settings'),[]);});

test('main chat contains only conversation, attachments and composer without introductory controls',()=>{
 AgentChat.statusError=null;
 AgentChat.statuses=[{agent:'api',available:true,personalSupported:true,detail:'Informação sobre sessões externas'}];
 const view=open();
 for(const selector of ['.chat-identity','.chat-advanced','.chat-storage','.chat-empty'])assert.equal(view.el.querySelector(selector),null,selector);
 assert.equal(view.el.find('button').some(button=>button.textContent==='Contexto'),false);
 assert.ok(view.el.querySelector('.chat-attachments'));
 assert.ok(view.el.querySelector('.chat-permission'));
 assert.ok(view.el.querySelector('.send-btn'));
 assert.doesNotMatch(view.el.querySelector('.chat-notice').textContent,/Informação sobre sessões externas|retomada no CLI/);
 assert.equal(view.el.find('button').filter(button=>button.textContent==='Ler e resumir').every(button=>button.hidden),true);
});
