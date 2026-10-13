import {h} from './dom';
import {State as OfficialState} from '../core/state';
import {State} from '../personal/core/state';
import {AgentChat, denyAgentChatApproval, registerAgentChatHandlers, setHermesDisplayName} from '../personal/core/agent-chat';
import {updatePersonalApproval} from '../core/personal-ui';
import {Bridge,onEvent,type ManagedModelStatus} from '../personal/core/bridge';
import {buildPrompt} from '../personal/views/chat';
import {buildLearned} from '../personal/views/learned';
import type {ViewHost} from './views';
import '../personal/style.css';
import {ingestConversationAttachments} from '../personal/core/attachments';
import {buildPicker} from './chat';
import {activeModel,providerDef} from '../core/providers';

export function buildPersonalAgent(onHeightChange:()=>void):ViewHost & {setActive(active:boolean):void} {
 let active=false; let learned=false;
 const chat=buildPrompt(onHeightChange,()=>{ learned=true; sync(); onHeightChange(); });
 const knowledge=buildLearned({setView(view){ learned=view==='learned'; sync(); onHeightChange(); }});
 chat.el.classList.remove('view');
 knowledge.el.classList.remove('view');
 const back=h('button',{class:'picker-chip',text:'Voltar ao agente',onclick:()=>{learned=false;sync();onHeightChange();}});
 const providers=h('div',{class:'picker-chips'});
 for (const agent of ['api','hermes','codex','claude'] as const) {
  const button=h('button',{class:'picker-chip',text:agent==='api'?'Coucou · provedor escolhido':agent==='hermes'?'Hermes / FreeLLMAPI':agent==='codex'?'Codex':'Claude Code',onclick:()=>{AgentChat.selectPersonal(agent);sync();onHeightChange();}});
  providers.append(button);
 }
 const modelButton=h('button',{class:'picker-chip agent-model-button',onclick:()=>{if(picker.isOpen)picker.close();else picker.open();}});
 const modelControls=h('div',{class:'agent-provider-controls'},modelButton);
 const picker=buildPicker(()=>{modelControls.classList.toggle('picking',picker.isOpen);OfficialState.notify();onHeightChange();});
 modelControls.append(picker.el);
 const el=h('div',{class:'personal-runtime'},providers,modelControls,back,chat.el,knowledge.el);
 AgentChat.selectPersonal('api');
 const imported=new Set<string>();
 function sync() {
  const api=AgentChat.provider==='api';
  modelControls.hidden=!api;
  modelButton.disabled=!!AgentChat.current?.runId;
  modelButton.textContent=`${providerDef(OfficialState.settings.chatProvider).name} · ${activeModel(OfficialState.settings)||'Escolher modelo'}`;
  for(const button of providers.children) (button as HTMLElement).classList.toggle('on',button.textContent===(AgentChat.provider==='api'?'Coucou · provedor escolhido':AgentChat.provider==='hermes'?'Hermes / FreeLLMAPI':AgentChat.provider==='codex'?'Codex':'Claude Code'));
  // The official drop has completed only after inbox ingestion. Import once into the personal scope.
  const file=OfficialState.droppedFile;
  if(active && file && /[\\/]inbox[\\/]/i.test(file.path) && !imported.has(file.path)) {
   imported.add(file.path);
   const conversation=AgentChat.current ?? AgentChat.selectPersonal('api');
   void ingestConversationAttachments(conversation.id,[file.path]).catch(()=>{}).finally(()=>{OfficialState.notify();onHeightChange();});
  }

  updatePersonalApproval(AgentChat.current?.approval?.requestId ?? null,active);
  State.settings={...State.settings,...OfficialState.settings,screen:OfficialState.settings.screen === "cursor" ? "cursor" : "primary"};
  State.mode=OfficialState.mode;
  State.view=active && OfficialState.view==='prompt' ? (learned?'learned':'prompt'):'overview';
  State.paused=OfficialState.paused;
  back.hidden=!learned; chat.el.hidden=learned; knowledge.el.hidden=!learned;
  chat.sync(); if(learned)knowledge.sync();
 }
 State.subscribe(()=>{ if(active){ sync();OfficialState.notify();onHeightChange(); } });
 void registerAgentChatHandlers({alert(){ if(active){ sync(); onHeightChange(); } }});
 const apply=(status:ManagedModelStatus)=>{setHermesDisplayName(status.settings.agentName);sync();};
 void Bridge.managedServiceStatus().then(apply).catch(()=>{});
 void onEvent<ManagedModelStatus>('managed-models-changed',apply);
 return {el,sync,focus(){if(!learned)chat.focus?.();},setActive(value){
  active=value;
  if(!value && AgentChat.current?.approval) void denyAgentChatApproval(AgentChat.current.id);
  sync();
 }};
}
