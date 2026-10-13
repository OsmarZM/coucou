import { Bridge, type HermesRuntimeStatus } from "../personal/core/bridge";
import { h, clear } from "../views/dom";

export async function buildHermesSection(): Promise<HTMLElement> {
  const body=h("div", {style:"display:flex;flex-direction:column;gap:12px"});
  const section=h("section",{},h("h2",{text:"Hermes — supervisor dos agentes"}),body);
  let status:HermesRuntimeStatus|null=null, busy=false, feedback="";
  async function operation(task:()=>Promise<HermesRuntimeStatus>, progress="Verificando configuração...", complete="Estado atualizado.") {
    if(busy)return;busy=true;feedback=progress;draw();
    try{status=await task();feedback=complete;}
    catch(error){feedback=String(error).replace(/^Error:\s*/,"");}finally{busy=false;draw();}
  }
  function draw() {
    clear(body);
    body.append(h("p",{class:"hint",text:"Escolha o personagem Hermes na guia principal para conversar com o supervisor. Ele compartilha o contexto pessoal e solicita autorização antes de criar auxiliares Codex ou Claude. Gemini e Copilot continuam bloqueados para chat até qualificar o isolamento."}));
    if(feedback)body.append(h("div",{class:"notice",role:"status",text:feedback}));
    if(!status){body.append(h("button",{text:"Consultar estado do Hermes",disabled:busy,onclick:()=>void operation(Bridge.hermesStatus)}));return;}
    const config=status.settings;
    const python=h("input",{type:"text",value:config.pythonExecutable,"aria-label":"Python privado do Hermes",style:"flex:1"});
    const source=h("input",{type:"text",value:config.sourceDirectory,"aria-label":"Origem fixada do Hermes",style:"flex:1"});
    const free=h("input",{type:"checkbox","aria-label":"Usar serviço gratuito configurado"});free.checked=config.useFreeModels;
    const enabled=h("input",{type:"checkbox","aria-label":"Ativar supervisor Hermes"});enabled.checked=config.enabled;
    const base=h("input",{type:"url",value:config.baseUrl,placeholder:"https://seu-endpoint/v1","aria-label":"Endpoint do modelo",style:"flex:1"});
    const model=h("input",{type:"text",value:config.model,placeholder:"Identificador exato do modelo","aria-label":"Modelo do Hermes",style:"flex:1"});
    const contextWindow=h("input",{type:"number",value:String(config.contextWindowTokens),min:"64000",max:"1000000",step:"1","aria-label":"Capacidade de contexto do modelo em tokens",style:"flex:1"});
    for(const field of [python,source,free,enabled,base,model,contextWindow])field.disabled=busy;
    body.append(h("div",{class:"row"},h("label",{text:"Python privado"}),python),h("div",{class:"row"},h("label",{text:"Origem Hermes"}),source));
    body.append(h("div",{class:"hint",text:`O setup instala Python e Hermes fixados em uma pasta privada. A revisão exigida é ${status.revision}. Perfis e logins globais são preservados; caminhos de revisões incompatíveis são recusados.`}));
    body.append(h("button",{text:"Instalar runtime Hermes fixado",disabled:busy||status.runtimeInstalled,onclick:()=>void operation(Bridge.hermesProvision,"Baixando e verificando Python e Hermes fixados. A primeira instalação pode demorar em disco lento; prazo máximo de 60 minutos. Nenhum modelo será consultado.","Runtime privado instalado. Configure o modelo e ative o supervisor quando desejar.")}));
    body.append(h("label",{class:"row"},free,h("span",{text:"Usar o serviço de modelos gratuitos configurado acima"})),h("label",{class:"row"},enabled,h("span",{text:"Ativar o supervisor Hermes"})));
    const custom=h("div",{});custom.hidden=free.checked;free.addEventListener("change",()=>{custom.hidden=free.checked;});
    custom.append(h("div",{class:"row"},h("label",{text:"Endpoint alternativo"}),base),h("div",{class:"row"},h("label",{text:"Modelo alternativo"}),model));
    const key=h("input",{type:"password",autocomplete:"off","aria-label":"Chave do modelo alternativo",placeholder:"Chave própria do modelo",style:"flex:1"});
    const keyMessage=h("span",{class:"hint"});
    const saveKey=h("button",{text:"Guardar chave",onclick:async()=>{const value=key.value.trim();key.value="";if(!value){keyMessage.textContent="Informe uma chave.";return;}saveKey.disabled=true;try{await Bridge.secretSet("hermes-api-key",value);keyMessage.textContent="Chave guardada no cofre do Windows.";}catch(e){keyMessage.textContent=String(e);}finally{saveKey.disabled=false;}}});
    custom.append(h("div",{class:"row"},key,saveKey,keyMessage));body.append(custom);
    body.append(h("div",{class:"row"},h("label",{text:"Contexto do modelo em tokens"}),contextWindow),h("p",{class:"hint",text:"O Hermes fixado exige pelo menos 64.000 tokens de contexto. Informe a capacidade documentada do modelo escolhido, inclusive no serviço gratuito. Este valor é configuração, não medição nem ampliação do limite do provedor."}));
    body.append(h("div",{class:"row"},h("button",{text:"Salvar supervisor",disabled:busy,onclick:()=>void operation(()=>Bridge.hermesConfigure({enabled:enabled.checked,pythonExecutable:python.value.trim(),sourceDirectory:source.value.trim(),useFreeModels:free.checked,baseUrl:base.value.trim(),model:model.value.trim(),contextWindowTokens:Number(contextWindow.value)}),"Salvando supervisor...","Configuração salva. A conexão e as ferramentas serão verificadas em cada turno.")}),h("button",{text:"Atualizar estado",disabled:busy,onclick:()=>void operation(Bridge.hermesStatus)})));
    body.append(h("div",{class:status.available?"notice":"notice warn",text:status.detail}));
    body.append(h("div",{class:"hint",text:"Memória pessoal é mantida pelo Coucou e espelhada no perfil privado do Hermes. Preferências explícitas de baixo risco são aprendidas na conversa; procedimentos e propostas inferidas podem ser revisados em Contexto. Ferramentas nativas que escapam às permissões do aplicativo ficam bloqueadas."}));
  }
  try{status=await Bridge.hermesStatus();}catch(e){feedback=String(e);}draw();return section;
}
