//! Bounded JSONL, cancellation and explicit one-time decisions for CLI clients.
use super::{
    process::{Executable, Job},
    Control, RunContext,
};
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::watch,
};

pub type Sink = Arc<dyn Fn(&str, Value) + Send + Sync>;
const MAX_FRAME: usize = 1024 * 1024;
const MAX_QUEUE: usize = 256;
const MAX_QUEUE_BYTES: usize = 8 * 1024 * 1024;

pub struct ProcessIo {
    child: Child,
    job: Job,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    queued: VecDeque<(Value, usize)>,
    queued_bytes: usize,
    counter: u64,
    cancel: watch::Receiver<bool>,
    control: Arc<Control>,
    ctx: RunContext,
    sink: Sink,
    stderr_task: tokio::task::JoinHandle<()>,
    memory: Option<crate::memory::MemoryService>,
    user_message: String,
    documents: Option<crate::documents::DocumentService>,
    attachment_ids: Vec<String>,
}
impl ProcessIo {
    pub fn spawn(
        executable: &Executable,
        args: &[String],
        ctx: &RunContext,
        cancel: watch::Receiver<bool>,
        control: Arc<Control>,
        sink: Sink,
    ) -> Result<Self, String> {
        let (mut child, job) = super::process::spawn(executable, args, &ctx.cwd)?;
        let stdin = child.stdin.take().ok_or("CLI stdin unavailable.")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("CLI stdout unavailable.")?);
        let mut stderr = child.stderr.take().ok_or("CLI stderr unavailable.")?;
        // Drain stderr without storing or logging credentials, prompts or raw output.
        let stderr_task = tokio::spawn(async move {
            let mut bytes = [0u8; 8192];
            while matches!(stderr.read(&mut bytes).await,Ok(n) if n>0) {}
        });
        let value = Self {
            child,
            job,
            stdin,
            stdout,
            queued: VecDeque::new(),
            queued_bytes: 0,
            counter: 0,
            cancel,
            control,
            ctx: ctx.clone(),
            sink,
            stderr_task,
            memory: None,
            user_message: ctx.query.clone(),
            documents: None,
            attachment_ids: Vec::new(),
        };
        value.emit("activity",json!({"kind":"process","processCount":value.job.process_count(),"capturedAt":crate::usage::now_seconds(),"source":"Windows JobObject.ActiveProcesses"}));
        Ok(value)
    }
    pub fn emit(&self, kind: &str, data: Value) {
        (self.sink)(kind, data);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.cancel.borrow()
    }
    pub fn context(&self) -> &RunContext {
        &self.ctx
    }
    pub fn cancel_receiver(&self) -> watch::Receiver<bool> {
        self.cancel.clone()
    }
    pub fn memory_service(&self) -> Option<crate::memory::MemoryService> {
        self.memory.clone()
    }
    pub fn set_memory_service(&mut self, memory: Option<crate::memory::MemoryService>) {
        self.memory = memory;
    }
    pub fn user_message(&self) -> &str {
        &self.user_message
    }
    pub fn set_user_message(&mut self, message: String) {
        self.user_message = message;
    }
    pub fn document_service(&self) -> Option<crate::documents::DocumentService> {
        self.documents.clone()
    }
    pub fn attachment_ids(&self) -> &[String] {
        &self.attachment_ids
    }
    pub fn set_document_service(
        &mut self,
        service: Option<crate::documents::DocumentService>,
        attachment_ids: Vec<String>,
    ) {
        self.documents = service;
        self.attachment_ids = attachment_ids;
    }
    pub async fn send(&mut self, mut message: Value) -> Result<(), String> {
        if self.ctx.agent == "codex" {
            if let Some(object) = message.as_object_mut() {
                object.remove("jsonrpc");
            }
        }
        let mut bytes = serde_json::to_vec(&message).map_err(|_| "Invalid CLI request.")?;
        if bytes.len() > MAX_FRAME {
            return Err("CLI request is too large.".into());
        }
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(5), self.stdin.write_all(&bytes))
            .await
            .map_err(|_| "CLI input timed out.")?
            .map_err(|_| "CLI input closed.".to_string())
    }
    async fn read_raw(&mut self, interruptible: bool) -> Result<Value, String> {
        if interruptible && self.is_cancelled() {
            return Err("Turn cancelled.".into());
        }
        let mut bytes = Vec::new();
        let reader = (&mut self.stdout).take((MAX_FRAME + 1) as u64);
        tokio::pin!(reader);
        let read = reader.read_until(b'\n', &mut bytes);
        let length = if interruptible {
            tokio::pin!(read);
            let deadline = tokio::time::sleep(Duration::from_secs(180));
            tokio::pin!(deadline);
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            loop {
                tokio::select! {biased;
                    _ = self.cancel.changed()=>return Err("Turn cancelled.".into()),
                    result=&mut read=>break result.map_err(|_|"Unable to read CLI output.")?,
                    _=&mut deadline=>return Err("CLI did not respond within the turn timeout.".into()),
                    _=tick.tick()=>(self.sink)("activity",json!({"kind":"process","processCount":self.job.process_count(),"capturedAt":crate::usage::now_seconds(),"source":"Windows JobObject.ActiveProcesses"})),
                }
            }
        } else {
            read.await.map_err(|_| "Unable to read CLI output.")?
        };
        if length == 0 {
            return Err(
                "CLI closed before completing its turn. Check its login and version in a terminal."
                    .into(),
            );
        }
        if length > MAX_FRAME || bytes.last() != Some(&b'\n') {
            return Err("CLI emitted an oversized or incomplete JSON frame.".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "CLI emitted invalid JSON. This version may be incompatible.".into())
    }
    pub async fn next(&mut self) -> Result<Value, String> {
        if self.is_cancelled() {
            return Err("Turn cancelled.".into());
        }
        if let Some((message, bytes)) = self.queued.pop_front() {
            self.queued_bytes -= bytes;
            return Ok(message);
        }
        self.read_raw(true).await
    }
    pub async fn next_uncancelled(&mut self, timeout: Duration) -> Result<Value, String> {
        if let Some((message, bytes)) = self.queued.pop_front() {
            self.queued_bytes -= bytes;
            return Ok(message);
        }
        tokio::time::timeout(timeout, self.read_raw(false))
            .await
            .map_err(|_| "CLI cancellation was not acknowledged.".to_string())?
    }
    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_response(method, params).await?
    }
    /// An optional capability may be absent at the RPC layer. A broken or
    /// timed-out transport always remains fatal; never continue a partial frame.
    pub async fn optional_request(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Option<Value>, String> {
        Ok(self.request_response(method, params).await?.ok())
    }
    async fn request_response(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Result<Value, String>, String> {
        if self.is_cancelled() {
            return Err("Turn cancelled.".into());
        }
        self.counter += 1;
        let id = json!(format!("coucou-{}", self.counter));
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let message = self.read_raw(true).await?;
                if message.get("id") == Some(&id) && message.get("method").is_none() {
                    if let Some(error) = message.get("error") {
                        let text = error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("CLI request failed.");
                        return Ok(Err(crate::privacy::diagnostic(text)));
                    }
                    return message
                        .get("result")
                        .cloned()
                        .map(Ok)
                        .ok_or_else(|| "CLI response has no result.".into());
                }
                if self.queued.len() >= MAX_QUEUE {
                    return Err("CLI initialization event queue exceeded its limit.".into());
                }
                let size = serde_json::to_vec(&message)
                    .map_err(|_| "Invalid CLI notification.".to_string())?
                    .len();
                if self.queued_bytes.saturating_add(size) > MAX_QUEUE_BYTES {
                    return Err("CLI initialization queue exceeded its byte limit.".into());
                }
                self.queued_bytes += size;
                self.queued.push_back((message, size));
            }
        })
        .await
        .map_err(|_| "CLI initialization/request timed out.".to_string())?
    }
    pub async fn approve(&mut self, details: Value) -> Result<String, String> {
        self.approval(details, false).await
    }
    pub async fn approve_scoped(&mut self, details: Value) -> Result<String, String> {
        self.approval(details, true).await
    }
    async fn approval(
        &mut self,
        details: Value,
        conversation_scope: bool,
    ) -> Result<String, String> {
        if self.is_cancelled() {
            return Ok("deny".into());
        }
        let title = details.get("title").and_then(Value::as_str).unwrap_or("");
        let detail = details.get("detail").and_then(Value::as_str).unwrap_or("");
        if title.is_empty()
            || detail.trim().is_empty()
            || title.len() > 256
            || detail.len() > 16384
            || detail.contains('\u{1b}')
            || detail.contains('\0')
        {
            self.emit(
                "status",
                json!({"text":"The permission request cannot be fully inspected and was denied."}),
            );
            return Ok("deny".into());
        }
        let (id, receiver) = self.control.insert_scoped(&self.ctx, conversation_scope)?;
        self.emit(
            "approval",
            json!({"requestId":id,"title":title,"detail":detail,"choices":if conversation_scope {vec!["allow","allowConversation","deny"]} else {vec!["allow","deny"]}}),
        );
        let decision = tokio::select! {biased;_=self.cancel.changed()=>"deny".to_string(),result=tokio::time::timeout(Duration::from_secs(120),receiver)=>result.ok().and_then(Result::ok).unwrap_or_else(||"deny".into())};
        self.control.remove(&id);
        self.emit("approvalResolved", json!({"requestId":id}));
        Ok(if self.is_cancelled() {
            "deny".into()
        } else {
            decision
        })
    }
    pub async fn shutdown(&mut self) {
        self.job.terminate();
        let _ = tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await;
        self.stderr_task.abort();
        self.emit("activity",json!({"kind":"process","processCount":0,"capturedAt":crate::usage::now_seconds(),"source":"Windows JobObject terminated"}));
    }
}
impl Drop for ProcessIo {
    fn drop(&mut self) {
        self.job.terminate();
        self.stderr_task.abort();
    }
}
