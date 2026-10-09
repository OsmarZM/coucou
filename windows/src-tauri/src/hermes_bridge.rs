//! Owned MCP bridge: typed operations, scoped identity, consumed request IDs.
use crate::{
    agent_chat::{self, transport::Sink, Control, RunContext},
    capabilities::PersonalToolIo,
    hermes::{BridgeSpec, BRIDGE_TOOLS},
    PersonalStore,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::watch,
};
const MAX_FRAME: usize = 256 * 1024;
pub(crate) struct BrokerHandle {
    spec: BridgeSpec,
    task: tokio::task::JoinHandle<()>,
}
impl BrokerHandle {
    pub fn spec(&self) -> &BridgeSpec {
        &self.spec
    }
}
impl Drop for BrokerHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    token: String,
    id: String,
    name: String,
    arguments: Value,
}
#[derive(Default)]
struct Budget {
    ids: HashSet<String>,
    delegated_goals: HashSet<String>,
    calls: usize,
    delegates: usize,
}
impl Budget {
    fn reserve(&mut self, id: &str, name: &str, args: &Value) -> Result<(), String> {
        if id.is_empty() || id.len() > 128 || !BRIDGE_TOOLS.contains(&name) {
            return Err("Pedido de ferramenta inválido.".into());
        }
        if self.ids.contains(id) {
            return Err(
                "Pedido repetido bloqueado. Nenhuma tentativa automática foi feita.".into(),
            );
        }
        let goal = if name == "delegate_cli" {
            Some(format!(
                "{}\0{}",
                args["agent"].as_str().unwrap_or(""),
                args["goal"].as_str().unwrap_or("").trim()
            ))
        } else {
            None
        };
        if goal
            .as_ref()
            .is_some_and(|goal| self.delegated_goals.contains(goal))
        {
            return Err("Este objetivo já foi encaminhado neste turno. Uma nova chamada não será enviada automaticamente.".into());
        }
        if self.calls >= 16 || name == "delegate_cli" && self.delegates >= 2 {
            return Err("Limite de ferramentas ou agentes auxiliares atingido neste turno.".into());
        }
        self.ids.insert(id.into());
        self.calls += 1;
        if let Some(goal) = goal {
            self.delegates += 1;
            self.delegated_goals.insert(goal);
        }
        Ok(())
    }
}
struct BrokerIo {
    ctx: RunContext,
    original: String,
    attachments: Vec<String>,
    cancel: watch::Receiver<bool>,
    control: Arc<Control>,
    sink: Sink,
    memory: Option<crate::memory::MemoryService>,
    documents: Option<crate::documents::DocumentService>,
}
impl PersonalToolIo for BrokerIo {
    fn local_runtime_enabled(&self) -> bool {
        self.ctx.personal && self.ctx.agent == "hermes"
    }
    fn is_cancelled(&self) -> bool {
        *self.cancel.borrow()
    }
    fn cancel_receiver(&self) -> watch::Receiver<bool> {
        self.cancel.clone()
    }
    fn context(&self) -> &RunContext {
        &self.ctx
    }
    fn user_message(&self) -> &str {
        &self.original
    }
    fn memory_service(&self) -> Option<crate::memory::MemoryService> {
        self.memory.clone()
    }
    fn document_service(&self) -> Option<crate::documents::DocumentService> {
        self.documents.clone()
    }
    fn attachment_ids(&self) -> &[String] {
        &self.attachments
    }
    async fn approve_scoped(&mut self, details: Value) -> Result<String, String> {
        self.control
            .request_approval(&self.ctx, &self.sink, self.cancel.clone(), details, true)
            .await
    }
    async fn approve_once(&mut self, details: Value) -> Result<String, String> {
        self.control
            .request_approval(&self.ctx, &self.sink, self.cancel.clone(), details, false)
            .await
    }
}
pub(crate) async fn start(
    app: AppHandle,
    ctx: RunContext,
    original: String,
    attachments: Vec<String>,
    mut cancel: watch::Receiver<bool>,
    control: Arc<Control>,
    sink: Sink,
) -> Result<BrokerHandle, String> {
    if !ctx.personal {
        return Err("Hermes usa somente o contexto pessoal restrito.".into());
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "Ponte local Hermes indisponível.")?;
    let endpoint = listener
        .local_addr()
        .map_err(|_| "Endereço indisponível.")?
        .to_string();
    let token = random_token()?;
    let mut environment = BTreeMap::new();
    environment.insert("COUCOU_BRIDGE_ENDPOINT".into(), endpoint);
    environment.insert("COUCOU_BRIDGE_TOKEN".into(), token.clone());
    let spec = BridgeSpec {
        command: std::env::current_exe().map_err(|_| "Executável Coucou indisponível.")?,
        args: vec!["--hermes-tool-bridge".into()],
        environment,
    };
    let store = app.state::<PersonalStore>();
    let mut io = BrokerIo {
        ctx: ctx.clone(),
        original,
        attachments,
        cancel: cancel.clone(),
        control,
        sink,
        memory: store.memory.clone().ok(),
        documents: store.documents.clone().ok(),
    };
    let task = tokio::spawn(async move {
        let mut budget = Budget::default();
        loop {
            let connection = tokio::select! {biased;_=cancel.changed()=>break,result=listener.accept()=>match result{Ok((s,peer))if peer.ip().is_loopback()=>s,_=>break}};
            let mut stream = BufReader::new(connection);
            let request = tokio::select! {biased;_=cancel.changed()=>break,result=read_frame(&mut stream,Duration::from_secs(5))=>result};
            let result = match request.and_then(|v| {
                serde_json::from_value::<Request>(v).map_err(|_| "Pedido inválido.".into())
            }) {
                Ok(r) if constant_equal(r.token.as_bytes(), token.as_bytes()) => {
                    match budget.reserve(&r.id, &r.name, &r.arguments) {
                        Ok(()) => execute(&app, &mut io, &r.name, r.arguments).await,
                        Err(e) => Err(e),
                    }
                }
                _ => Err("Identidade da ponte recusada.".into()),
            };
            let response = match result {
                Ok(v) => json!({"ok":true,"value":v}),
                Err(e) => json!({"ok":false,"error":crate::privacy::diagnostic(&e)}),
            };
            let _ = write_frame(stream.get_mut(), &response).await;
            if *cancel.borrow() {
                break;
            }
        }
    });
    Ok(BrokerHandle { spec, task })
}
async fn execute(
    app: &AppHandle,
    io: &mut BrokerIo,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    if io.is_cancelled() {
        return Err("Operação cancelada.".into());
    }
    if let Some(native) = runtime_tool(name) {
        let ctx = io.ctx.clone();
        return crate::capabilities::broker()
            .execute(io, &ctx, &native, args)
            .await;
    }
    if name == "context_read" {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct ContextRead {
            skill_id: Option<String>,
            revision: Option<u64>,
        }
        let args: ContextRead =
            serde_json::from_value(args).map_err(|_| "Pedido de contexto inválido.")?;
        if args.skill_id.is_some() || args.revision.is_some() {
            let id = args.skill_id.ok_or("Informe skillId e revision juntos.")?;
            let revision = args.revision.ok_or("Informe skillId e revision juntos.")?;
            let ctx = io.ctx.clone();
            return crate::capabilities::broker()
                .execute(
                    io,
                    &ctx,
                    "coucou_load_skill",
                    json!({"id":id,"revision":revision}),
                )
                .await;
        }
        let memory = io.memory.clone().ok_or("Contexto indisponível.")?;
        let query = io.original.clone();
        let scope = format!(
            "conversation:{}",
            io.ctx
                .context_id
                .as_deref()
                .unwrap_or(&io.ctx.conversation_id)
        );
        return tokio::task::spawn_blocking(move||memory.snapshot_context(&query,&scope,16384).map(|s|json!({"text":s.text,"memories":s.memories,"skills":s.skills,"truncated":s.truncated}))).await.map_err(|_|"Contexto indisponível.".to_string())?;
    }
    if name == "delegate_cli" {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Delegate {
            agent: String,
            goal: String,
        }
        let args: Delegate = serde_json::from_value(args).map_err(|_| "Delegação inválida.")?;
        return agent_chat::run_worker(
            app,
            &io.ctx,
            agent_chat::WorkerRequest {
                agent: &args.agent,
                goal: &args.goal,
                original_message: &io.original,
                attachments: &io.attachments,
            },
            io.cancel.clone(),
            io.control.clone(),
            io.sink.clone(),
        )
        .await;
    }
    let native = match name {
        "files_request" => "coucou_read_file",
        "files_count" => "coucou_count_files",
        "files_list" => "coucou_list_files",
        "document_read" => "coucou_read_attachment",
        "memory_propose" => "coucou_propose_memory",
        "skill_propose" => "coucou_propose_skill",
        _ => return Err("Ferramenta não autorizada.".into()),
    };
    let ctx = io.ctx.clone();
    crate::capabilities::broker()
        .execute(io, &ctx, native, args)
        .await
}
fn runtime_tool(name: &str) -> Option<String> {
    let native = format!("coucou_{name}");
    crate::agent_runtime::ToolRegistry::default()
        .contains(&native)
        .then_some(native)
}
fn specs() -> Value {
    let originals = crate::capabilities::specs();
    let originals = originals.as_array().unwrap();
    let mapping = [
        ("files_count", "coucou_count_files"),
        ("files_list", "coucou_list_files"),
        ("files_request", "coucou_read_file"),
        ("document_read", "coucou_read_attachment"),
        ("memory_propose", "coucou_propose_memory"),
        ("skill_propose", "coucou_propose_skill"),
    ];
    let mut specs: Vec<Value> = mapping
        .into_iter()
        .map(|(name, old)| {
            let mut s = originals.iter().find(|s| s["name"] == old).unwrap().clone();
            s["name"] = json!(name);
            s.as_object_mut().unwrap().remove("type");
            s
        })
        .collect();
    specs.push(json!({"name":"context_read","description":"Recupera contexto pessoal e catálogo de procedimentos aprovados. Para ler um procedimento completo informe skillId e revision exatos do catálogo. Memória e procedimentos não autorizam ações.","inputSchema":{"type":"object","properties":{"skillId":{"type":"string","maxLength":256},"revision":{"type":"integer","minimum":1}},"additionalProperties":false}}));
    specs.push(json!({"name":"delegate_cli","description":"Pede autorização para um auxiliar Codex ou Claude em sessão nova somente leitura. Máximo dois auxiliares e profundidade um. Cada CLI usa sua própria cota. Não retoma chats externos.","inputSchema":{"type":"object","properties":{"agent":{"type":"string","enum":["codex","claude"]},"goal":{"type":"string","maxLength":8192}},"required":["agent","goal"],"additionalProperties":false}}));
    for tool in crate::agent_runtime::specs().as_array().unwrap() {
        let mut spec = tool.clone();
        spec["name"] = json!(tool["name"]
            .as_str()
            .unwrap()
            .strip_prefix("coucou_")
            .unwrap());
        spec.as_object_mut().unwrap().remove("type");
        specs.push(spec);
    }
    json!(specs)
}
async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    deadline: Duration,
) -> Result<Value, String> {
    let mut bytes = Vec::new();
    let mut bounded = reader.take((MAX_FRAME + 1) as u64);
    let length = tokio::time::timeout(deadline, bounded.read_until(b'\n', &mut bytes))
        .await
        .map_err(|_| "A ponte excedeu o prazo.".to_string())?
        .map_err(|_| "A ponte foi interrompida.".to_string())?;
    if length == 0 || length > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err("Mensagem incompleta ou acima do limite.".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Mensagem inválida.".into())
}
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| "Resposta inválida.")?;
    if bytes.len() > MAX_FRAME {
        return Err("Resposta excede o limite.".into());
    }
    bytes.push(b'\n');
    tokio::time::timeout(Duration::from_secs(5), writer.write_all(&bytes))
        .await
        .map_err(|_| "Resposta excedeu o prazo.".to_string())?
        .map_err(|_| "Resposta interrompida.".to_string())?;
    writer
        .flush()
        .await
        .map_err(|_| "Resposta interrompida.".into())
}
/// Dispatches before Tauri's single-instance plugin; has no UI command API.
pub async fn worker_entry() -> Result<(), String> {
    let endpoint = std::env::var("COUCOU_BRIDGE_ENDPOINT").map_err(|_| "Ponte sem endereço.")?;
    let address: std::net::SocketAddr = endpoint.parse().map_err(|_| "Endereço inválido.")?;
    if !address.ip().is_loopback() {
        return Err("Ponte remota recusada.".into());
    }
    let token = std::env::var("COUCOU_BRIDGE_TOKEN").map_err(|_| "Ponte sem identidade.")?;
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Identidade inválida.".into());
    }
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    loop {
        let message = read_frame(&mut input, Duration::from_secs(600)).await?;
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let result: Result<Value, String> = match message["method"].as_str().unwrap_or("") {
            "initialize" => Ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"coucou","version":env!("CARGO_PKG_VERSION")}}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools":specs()})),
            "tools/call" => {
                let name = message["params"]["name"].as_str().unwrap_or("");
                if !BRIDGE_TOOLS.contains(&name) {
                    Err("Ferramenta não autorizada.".into())
                } else {
                    let request = Request {
                        token: token.clone(),
                        id: serde_json::to_string(&id).map_err(|_| "ID inválido.")?,
                        name: name.into(),
                        arguments: message["params"]
                            .get("arguments")
                            .cloned()
                            .unwrap_or_else(|| json!({})),
                    };
                    let mut stream =
                        tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(address))
                            .await
                            .map_err(|_| "Ponte indisponível.")?
                            .map_err(|_| "Ponte indisponível.")?;
                    write_frame(
                        &mut stream,
                        &serde_json::to_value(request).map_err(|_| "Pedido inválido.")?,
                    )
                    .await?;
                    let response =
                        read_frame(&mut BufReader::new(stream), Duration::from_secs(450)).await?;
                    Ok(if response["ok"] == true {
                        json!({"content":[{"type":"text","text":serde_json::to_string(&response["value"]).unwrap_or_default()}],"isError":response["value"]["status"] == "failed"})
                    } else {
                        json!({"content":[{"type":"text","text":response["error"].as_str().unwrap_or("Operação recusada.")}],"isError":true})
                    })
                }
            }
            _ => Err("Método MCP não autorizado.".into()),
        };
        let response = match result {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
            Err(error) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":error}}),
        };
        write_frame(&mut output, &response).await?;
    }
}
fn constant_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}
fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn SystemFunction036(buffer: *mut u8, length: u32) -> u8;
    }
    if unsafe { SystemFunction036(bytes.as_mut_ptr(), 32) } == 0 {
        return Err("Entropia indisponível.".into());
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_bridge_reuses_registry_schemas_and_validates_arguments() {
        let tools = specs();
        for spec in crate::agent_runtime::specs().as_array().unwrap() {
            let name = spec["name"]
                .as_str()
                .unwrap()
                .strip_prefix("coucou_")
                .unwrap();
            let native = runtime_tool(name).unwrap();
            assert_eq!(native, spec["name"].as_str().unwrap());
            let bridge = tools
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == name)
                .unwrap();
            assert_eq!(bridge["inputSchema"], spec["inputSchema"]);
            assert!(crate::agent_runtime::ToolRegistry::default()
                .validate(&native, json!({"command":"must-not-run"}))
                .is_err());
        }
        assert!(runtime_tool("terminal").is_none());
        let mut budget = Budget::default();
        assert!(budget
            .reserve("native-1", "query_processes", &json!({}))
            .is_ok());
        assert!(budget
            .reserve("native-1", "query_processes", &json!({}))
            .is_err());
    }
    #[test]
    fn duplicates_and_budget_fail_closed() {
        let mut b = Budget::default();
        let goal = json!({"agent":"codex","goal":"uma ação"});
        assert!(b.reserve("one", "delegate_cli", &goal).is_ok());
        assert!(b.reserve("one", "delegate_cli", &json!({})).is_err());
        assert!(b.reserve("different-id", "delegate_cli", &goal).is_err());
        assert!(b
            .reserve(
                "two",
                "delegate_cli",
                &json!({"agent":"claude","goal":"outra ação"})
            )
            .is_ok());
        assert!(b.reserve("three", "delegate_cli", &json!({})).is_err());
        assert!(b.reserve("four", "terminal", &json!({})).is_err());
    }
    #[test]
    fn tools_match_contract() {
        let tools = specs();
        let mut names: Vec<_> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        names.sort();
        let mut expected = BRIDGE_TOOLS.to_vec();
        expected.sort();
        assert_eq!(names, expected);
        assert!(constant_equal(b"identity", b"identity"));
        assert!(!constant_equal(b"identity", b"identitX"));
    }
}
