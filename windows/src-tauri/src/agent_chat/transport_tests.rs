//! Synthetic subprocess checks. These fixtures never invoke an agent or model.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tokio::sync::watch;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

use super::{process::Executable, transport::ProcessIo, transport::Sink, Control, RunContext};

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    base: PathBuf,
    directory: PathBuf,
    script: PathBuf,
}

impl Fixture {
    fn new(script: &str) -> Self {
        let base = std::env::temp_dir()
            .canonicalize()
            .expect("The system temporary directory must be accessible for process tests.");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = base.join(format!(
            "coucou-chat-fixture-{}-{stamp}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("Cannot create an isolated CLI fixture directory.");
        let script_path = directory.join("fixture.cjs");
        fs::write(&script_path, script).expect("Cannot write the synthetic CLI fixture.");
        Self {
            base,
            directory,
            script: script_path,
        }
    }

    fn executable(&self) -> Executable {
        let program = std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join("node.exe"))
            .find(|candidate| candidate.is_file())
            .and_then(|candidate| candidate.canonicalize().ok())
            .expect("node.exe must be on PATH: the existing frontend build and synthetic process checks require Node.");
        Executable {
            program: program.clone(),
            prefix: vec![super::process::cli_path(&self.script)
                .to_string_lossy()
                .into_owned()],
            display: program,
        }
    }

    fn context(&self, agent: &str) -> RunContext {
        RunContext {
            conversation_id: "fixture-conversation".into(),
            run_id: "fixture-run".into(),
            agent: agent.into(),
            cwd: self.directory.clone(),
            session_id: None,
            query: "synthetic fixture".into(),
            writable: false,
            personal: false,
            context_id: None,
        }
    }

    fn spawn(
        &self,
        agent: &str,
        control: Arc<Control>,
        sink: Sink,
    ) -> (ProcessIo, watch::Sender<bool>) {
        let (sender, receiver) = watch::channel(false);
        let io = ProcessIo::spawn(
            &self.executable(),
            &[],
            &self.context(agent),
            receiver,
            control,
            sink,
        )
        .expect("The synthetic Node process must start inside its Windows Job.");
        (io, sender)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Recursively remove only this owned, resolved fixture directory.
        // Never follow a computed target outside the recorded temporary base.
        if let Ok(target) = self.directory.canonicalize() {
            if target.parent() == Some(self.base.as_path())
                && target
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("coucou-chat-fixture-"))
            {
                let _ = fs::remove_dir_all(target);
            }
        }
    }
}

fn silent_sink() -> Sink {
    Arc::new(|_, _| {})
}

#[tokio::test]
async fn optional_rpc_errors_remain_optional_but_a_broken_stream_is_fatal() {
    let fixture = Fixture::new(
        r#"const rl=require('node:readline').createInterface({input:process.stdin});rl.on('line',line=>{const r=JSON.parse(line);if(r.method==='broken')process.stdout.write('invalid JSON\n');else if(r.method==='missing')process.stdout.write(JSON.stringify({id:r.id,error:{code:-32601,message:'Capability unavailable'}})+'\n');else process.stdout.write(JSON.stringify({id:r.id,result:{ok:true}})+'\n');});"#,
    );
    let (mut io, _cancel) = fixture.spawn("codex", Arc::new(Control::default()), silent_sink());
    assert!(io
        .optional_request("missing", json!({}))
        .await
        .unwrap()
        .is_none());
    assert_eq!(io.request("ok", json!({})).await.unwrap()["ok"], true);
    assert!(io.optional_request("broken", json!({})).await.is_err());
    io.shutdown().await;
}

#[tokio::test]
async fn request_preserves_notification_order_and_uses_each_wire_envelope() {
    let fixture = Fixture::new(
        r#"
const rl = require('node:readline').createInterface({input: process.stdin});
const write = value => process.stdout.write(JSON.stringify(value) + '\n');
rl.on('line', line => {
  const request = JSON.parse(line);
  write({method: 'fixture/notification', params: {sequence: 1}});
  write({method: 'fixture/notification', params: {sequence: 2}});
  write({id: request.id, result: {
    method: request.method,
    jsonrpcPresent: Object.prototype.hasOwnProperty.call(request, 'jsonrpc'),
    jsonrpc: request.jsonrpc ?? null
  }});
});
"#,
    );
    for agent in ["codex", "gemini"] {
        let (mut io, _cancel) = fixture.spawn(agent, Arc::new(Control::default()), silent_sink());
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            io.request("initialize", json!({ "client": "fixture" })),
        )
        .await
        .expect("Synthetic initialization must respond promptly.")
        .unwrap();
        assert_eq!(response["method"], "initialize");
        assert_eq!(response["jsonrpcPresent"], agent == "gemini");
        assert_eq!(
            response["jsonrpc"],
            if agent == "gemini" {
                json!("2.0")
            } else {
                Value::Null
            }
        );
        assert_eq!(io.next().await.unwrap()["params"]["sequence"], 1);
        assert_eq!(io.next().await.unwrap()["params"]["sequence"], 2);
        io.shutdown().await;
    }
}

#[tokio::test]
async fn oversized_incoming_and_outgoing_frames_are_rejected_without_raw_echo() {
    let fixture = Fixture::new(
        r#"
process.stdout.write(JSON.stringify({message: 'x'.repeat(1024 * 1024 + 1)}) + '\n');
setInterval(() => {}, 1000);
"#,
    );
    let (mut io, _cancel) = fixture.spawn("codex", Arc::new(Control::default()), silent_sink());
    let request = json!({ "params": { "message": "s".repeat(1024 * 1024 + 1) } });
    let error = io.send(request).await.unwrap_err();
    assert_eq!(error, "CLI request is too large.");
    let error = tokio::time::timeout(Duration::from_secs(5), io.next())
        .await
        .expect("An oversized frame must fail promptly.")
        .unwrap_err();
    assert!(error.contains("oversized"));
    assert!(error.len() < 200);
    io.shutdown().await;
}

#[tokio::test]
async fn cancelling_a_pending_permission_denies_and_revokes_its_request() {
    let fixture = Fixture::new("setInterval(() => {}, 1000);\n");
    let control = Arc::new(Control::default());
    let (cancel, receiver) = watch::channel(false);
    let events = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let recorded = events.clone();
    let trigger = cancel.clone();
    let sink: Sink = Arc::new(move |kind, value| {
        recorded.lock().unwrap().push((kind.into(), value));
        if kind == "approval" {
            trigger.send(true).unwrap();
        }
    });
    let ctx = fixture.context("gemini");
    let mut io = ProcessIo::spawn(
        &fixture.executable(),
        &[],
        &ctx,
        receiver,
        control.clone(),
        sink,
    )
    .unwrap();
    let decision = tokio::time::timeout(
        Duration::from_secs(5),
        io.approve(json!({ "title": "Read fixture", "detail": "D:/fixture/README.md" })),
    )
    .await
    .expect("Cancellation must resolve the permission promptly.")
    .unwrap();
    assert_eq!(decision, "deny");
    assert!(io.is_cancelled());
    assert!(io.next().await.is_err());
    let events = events.lock().unwrap().clone();
    let request = events.iter().find(|(kind, _)| kind == "approval").unwrap();
    let id = request.1["requestId"].as_str().unwrap();
    assert!(!control.decide(&ctx.conversation_id, &ctx.run_id, id, "allow"));
    assert!(control.decisions.lock().unwrap().is_empty());
    assert!(events
        .iter()
        .any(|(kind, value)| kind == "approvalResolved" && value["requestId"] == id));
    io.shutdown().await;
}

struct ObservedProcess(HANDLE);

impl ObservedProcess {
    fn open(pid: u32) -> Self {
        Self(
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
                .expect("The synthetic parent and child must be observable."),
        )
    }

    fn running(&self) -> bool {
        let mut exit_code = 0;
        unsafe { GetExitCodeProcess(self.0, &mut exit_code) }
            .expect("Cannot inspect the synthetic process exit state.");
        exit_code == 259 // STILL_ACTIVE
    }
}

impl Drop for ObservedProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[tokio::test]
async fn shutdown_terminates_the_windows_job_parent_and_descendant() {
    let fixture = Fixture::new(
        r#"
if (process.argv.includes('--child')) {
  process.stdout.write(JSON.stringify({childPid: process.pid}) + '\n');
  setInterval(() => {}, 1000);
} else {
const cp = require('node:child_process');
const child = cp.spawn(process.execPath, [process.argv[1], '--child'], {
  windowsHide: true, stdio: ['ignore', 'pipe', 'ignore']
});
require('node:readline').createInterface({input: child.stdout}).once('line', line => {
  const ready = JSON.parse(line);
  process.stdout.write(JSON.stringify({parentPid: process.pid, childPid: ready.childPid}) + '\n');
});
setInterval(() => {}, 1000);
}
"#,
    );
    // One script can take either role. The child announces readiness only after
    // it has actually started; this avoids testing a not-yet-running PID.
    let (mut io, _cancel) = fixture.spawn("codex", Arc::new(Control::default()), silent_sink());
    let ready = tokio::time::timeout(Duration::from_secs(5), io.next())
        .await
        .expect("The synthetic descendant must announce readiness.")
        .unwrap();
    let parent = ObservedProcess::open(ready["parentPid"].as_u64().unwrap() as u32);
    let child = ObservedProcess::open(ready["childPid"].as_u64().unwrap() as u32);
    assert!(parent.running() && child.running());
    io.shutdown().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while parent.running() || child.running() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Windows Job shutdown must terminate every synthetic descendant.");
    assert!(!parent.running());
    assert!(!child.running());
}
