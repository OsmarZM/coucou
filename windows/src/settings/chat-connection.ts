import {h,clear} from '../views/dom';
import {Bridge} from '../core/bridge';
import {PROVIDERS,activeModel,providerDef,withModel,type ProviderId} from '../core/providers';
import type {Settings} from '../core/state';

/** Configuration owns the connection; rendering the chat never chooses an account. */
export function buildChatConnectionSection(current:()=>Settings,committed:(settings:Settings)=>void):HTMLElement {
 const provider=h('select',{'aria-label':'Provedor do chat automático'}) as HTMLSelectElement;
 for(const item of PROVIDERS) provider.append(h('option',{value:item.id,text:item.name}));
 provider.value=current().chatProvider;
 const model=h('input',{type:'text',value:activeModel(current()),'aria-label':'Modelo do chat automático',placeholder:'Identificador do modelo',spellcheck:'false',style:'flex:1;min-width:160px'}) as HTMLInputElement;
 const offered=h('select',{'aria-label':'Modelos disponíveis',hidden:'true',style:'flex:1;min-width:160px'}) as HTMLSelectElement;
 const feedback=h('div',{class:'hint',role:'status'});
 const load=h('button',{text:'Consultar modelos'});
 const save=h('button',{text:'Salvar conexão do chat'});
 let ticket=0;let busy=false;
 function lock(value:boolean){busy=value;provider.disabled=value;model.disabled=value;load.disabled=value;save.disabled=value;}
 provider.addEventListener('change',()=>{ticket++;offered.hidden=true;clear(offered);model.value=activeModel({...current(),chatProvider:provider.value as ProviderId});feedback.textContent='Escolha o modelo e salve para aplicar. As credenciais são configuradas nas seções abaixo.';});
 offered.addEventListener('change',()=>{model.value=offered.value;});
 load.addEventListener('click',async()=>{
  if(busy)return;const request=++ticket;const id=provider.value;lock(true);feedback.textContent='Consultando os modelos desta conexão…';
  try{
   const definition=providerDef(id);
   if(definition.key && !(await Bridge.secretPresent(definition.key)))throw new Error('Configure a chave deste provedor nas seções abaixo antes de consultar seus modelos.');
   if(definition.urlField && !current()[definition.urlField])throw new Error('Conecte este servidor na seção Modelos locais antes de consultar os modelos.');
   const models=await Bridge.chatModels(id);
   if(request!==ticket)return;
   clear(offered);for(const item of models)offered.append(h('option',{value:item.id,text:item.label}));
   offered.hidden=models.length===0;
   if(models.length){offered.value=models.some(m=>m.id===model.value)?model.value:models[0].id;model.value=offered.value;}
   feedback.textContent=models.length?'Selecione o modelo e salve a conexão.':'Este provedor não retornou modelos.';
  }catch(error){if(request===ticket)feedback.textContent=String(error).replace(/^Error:\s*/,'');}
  finally{lock(false);}
 });
 save.addEventListener('click',async()=>{
  if(busy)return;lock(true);
  try{
   const value=model.value.trim();
   if(!value || value.length>256 || /[\u0000-\u001f\u007f]/.test(value))throw new Error('Informe um identificador de modelo válido, com até 256 caracteres.');
   const id=provider.value as ProviderId;
   if(!PROVIDERS.some(p=>p.id===id))throw new Error('Provedor inválido.');
   const definition=providerDef(id);
   if(definition.urlField && !current()[definition.urlField])throw new Error('Configure o endereço na seção Modelos locais antes de salvar esta conexão.');
   const next=withModel({...current(),chatProvider:id},id,value);
   await Bridge.saveSettings(next);committed(next);feedback.textContent='Conexão salva. O chat em Automático usa esta configuração.';
  }catch(error){feedback.textContent=String(error).replace(/^Error:\s*/,'');}
  finally{lock(false);}
 });
 return h('section',{},h('h2',{text:'Conexão do chat automático'}),h('div',{class:'hint',text:'Provedor e modelo são definidos somente aqui. O chat usa esta conexão; não alterna contas nem faz fallback automaticamente.'}),h('div',{class:'row'},h('label',{text:'Provedor e modelo'}),provider,model),h('div',{class:'row'},load,offered,save),feedback);
}
