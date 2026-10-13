// Isolated visual fixture: no provider, file access or CLI calls.
import '../src/style.css';
import '../src/personal/style.css';
import {buildPrompt} from '../src/personal/views/chat';
import {AgentChat} from '../src/personal/core/agent-chat';
import {State} from '../src/personal/core/state';
AgentChat.provider='api';
AgentChat.statuses=[{agent:'api',available:true,personalSupported:true,path:null,detail:'Informação que não deve aparecer'}];
State.mode='expanded';State.view='prompt';
const view=buildPrompt(()=>{},()=>{},true);
view.el.classList.remove('view');
document.getElementById('preview')!.append(view.el);
view.sync();
