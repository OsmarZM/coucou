import {h} from './dom';
import {State as OfficialState} from '../core/state';
import {State} from '../personal/core/state';
import {AgentChat, registerAgentChatHandlers, setHermesDisplayName} from '../personal/core/agent-chat';
import {Bridge,onEvent,type ManagedModelStatus} from '../personal/core/bridge';
import {buildPrompt} from '../personal/views/chat';
import {buildLearned} from '../personal/views/learned';
import type {ViewHost} from './views';
import '../personal/style.css';

export function buildPersonalAgent(onHeightChange:()=>void):ViewHost & {setActive(active:boolean):void} {
 let active=false; let learned=false;
 const chat=buildPrompt(onHeightChange,()=>{ learned=true; sync(); onHeightChange(); });
 const knowledge=buildLearned({setView(view){ learned=view==='learned'; sync(); onHeightChange(); }});
 chat.el.classList.remove('view');
 knowledge.el.classList.remove('view');
 const back=h('button',{class:'picker-chip',text:'Voltar ao agente',onclick:()=>{learned=false;sync();onHeightChange();}});
 const providers=h('div',{class:'picker-chips'});
 for (const agent of ['hermes','codex','claude'] as const) {
  const button=h('button',{class:'picker-chip',text:agent==='hermes'?'Hermes / FreeLLMAPI':agent==='codex'?'Codex':'Claude Code',onclick:()=>{AgentChat.selectPersonal(agent);sync();onHeightChange();}});
  providers.append(button);
 }
 const el=h('div',{class:'personal-runtime'},providers,back,chat.el,knowledge.el);
 AgentChat.selectPersonal('hermes');
 function sync() {
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
 return {el,sync,focus(){if(!learned)chat.focus?.();},setActive(value){active=value;sync();}};
}
