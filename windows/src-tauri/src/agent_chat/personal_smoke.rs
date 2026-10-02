//! Opt-in native protocol qualification. The approver is a fixture, not the UI.
use super::*;
use std::{
    fs,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    root: std::path::PathBuf,
    base: std::path::PathBuf,
    conversation: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        crate::capabilities::broker().revoke(&self.conversation);
        if let Ok(resolved) = self.root.canonicalize() {
            // Resolve and verify the exact owned fixture before recursive deletion.
            if resolved.parent() == Some(self.base.as_path())
                && resolved == self.root
                && resolved.file_name().is_some_and(|name| {
                    name.to_string_lossy().starts_with("coucou-personal-smoke-")
                })
            {
                let _ = fs::remove_dir_all(resolved);
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "Uses two real ChatGPT plan turns with isolated fixtures; set COUCOU_RUN_PERSONAL_SMOKE=1"]
async fn personal_tools_are_scoped_and_resume_without_a_native_environment() {
    assert_eq!(
        std::env::var("COUCOU_RUN_PERSONAL_SMOKE").as_deref(),
        Ok("1")
    );
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().canonicalize().unwrap();
    let root = base.join(format!(
        "coucou-personal-smoke-{}-{nonce}",
        std::process::id()
    ));
    let conversation = format!("personal-smoke-{nonce}");
    let _fixture = Fixture {
        root: root.clone(),
        base: base.clone(),
        conversation: conversation.clone(),
    };
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("counted")).unwrap();
    fs::create_dir(root.join("counted/sub")).unwrap();
    fs::write(root.join("counted/a.txt"), "text fixture").unwrap();
    fs::write(root.join("counted/b.md"), "text fixture").unwrap();
    fs::write(root.join("counted/.hidden.txt"), "text fixture").unwrap();
    fs::write(root.join("counted/sub/nested.txt"), "text fixture").unwrap();
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN},
        };
        let hidden: Vec<u16> = root
            .join("counted/.hidden.txt")
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe { SetFileAttributesW(PCWSTR(hidden.as_ptr()), FILE_ATTRIBUTE_HIDDEN) }.unwrap();
    }
    let canary = format!("COUCOU_NEVER_READ_{nonce}");
    fs::write(root.join("not-authorized.txt"), &canary).unwrap();
    let counted = process::cli_path(&root.join("counted"));
    let denied = process::cli_path(&root.join("not-authorized.txt"));
    let control = Arc::new(Control::default());
    let events = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let approvals = Arc::new(Mutex::new((0usize, 0usize)));
    let executable = process::resolve("codex").unwrap();
    let mut ctx=RunContext{conversation_id:conversation.clone(),run_id:format!("first-{nonce}"),agent:"codex".into(),cwd:process::cli_path(&root),session_id:None,personal:true,context_id:Some("personal-main".into()),writable:false,
        query:format!("Conte os arquivos diretamente nesta pasta, incluindo ocultos e sem subpastas: {}. Você deve usar coucou_count_files, sem presumir a resposta. Depois responda exatamente {{\"arquivos\":N,\"subpastas\":N}}.",counted.display())};
    let mut native_session = None;
    for turn in 0..2 {
        if turn == 1 {
            ctx.run_id = format!("second-{nonce}");
            ctx.session_id = native_session.clone();
            ctx.query=format!("Use coucou_count_files novamente na pasta {} sem recursão. Depois use coucou_read_file para ler {}. Se a autorização for negada, informe a negativa e não invente o conteúdo.",counted.display(),denied.display());
        }
        let capture = events.clone();
        let approve = control.clone();
        let scope = ctx.clone();
        let folder = counted.clone();
        let tally = approvals.clone();
        let sink: transport::Sink = Arc::new(move |kind, data| {
            capture
                .lock()
                .unwrap()
                .push((kind.to_owned(), data.clone()));
            if kind == "approval" {
                let detail = data["detail"].as_str().unwrap_or("");
                let target = detail.lines().find_map(|line| line.strip_prefix("Alvo: "));
                let approved = target
                    .map(|target| process::cli_path(std::path::Path::new(target)))
                    .is_some_and(|target| target == folder)
                    && detail.contains("contar arquivos");
                let choice = if approved {
                    "allowConversation"
                } else {
                    "deny"
                };
                assert!(approve.decide(
                    &scope.conversation_id,
                    &scope.run_id,
                    data["requestId"].as_str().unwrap(),
                    choice
                ));
                let mut counts = tally.lock().unwrap();
                if approved {
                    counts.0 += 1;
                } else {
                    counts.1 += 1;
                }
            }
        });
        let (_cancel, receiver) = watch::channel(false);
        let mut io = transport::ProcessIo::spawn(
            &executable,
            &["app-server".into(), "--listen".into(), "stdio://".into()],
            &ctx,
            receiver,
            control.clone(),
            sink,
        )
        .unwrap();
        let result = codex::run_inner(&mut io, &ctx, true).await;
        let environment_snapshot = if let Ok(outcome) = &result {
            Some(
                io.request(
                    "thread/read",
                    json!({"threadId":outcome.session_id,"includeTurns":false}),
                )
                .await,
            )
        } else {
            None
        };
        io.shutdown().await;
        let outcome = result.unwrap_or_else(|error| {
            panic!(
                "Native personal protocol failed: {}",
                crate::privacy::diagnostic(&error)
            )
        });
        assert_eq!(outcome.status, "completed");
        let environment_snapshot = environment_snapshot
            .unwrap()
            .expect("Owned thread metadata must be readable");
        assert_eq!(environment_snapshot.pointer("/thread/environments"),Some(&json!([])),"Every completed personal turn must have zero native environments, including cold resume");
        assert!(!outcome.text.contains(&canary));
        println!(
            "Native fixture turn {}: approvals {:?}; answer {}",
            turn + 1,
            *approvals.lock().unwrap(),
            crate::privacy::diagnostic(&outcome.text.chars().take(1800).collect::<String>())
        );
        if turn == 0 {
            // A turn may include a separate commentary message before its final
            // JSON answer. Validate the final result, not the combined transcript.
            let parsed: Value = outcome
                .text
                .lines()
                .rev()
                .find_map(|line| {
                    serde_json::from_str::<Value>(line.trim())
                        .ok()
                        .filter(|value| value.is_object())
                })
                .expect("Expected bounded final JSON count result");
            assert_eq!(parsed["arquivos"], 3);
            assert_eq!(parsed["subpastas"], 1);
            native_session = Some(outcome.session_id);
        } else {
            assert_eq!(native_session.as_deref(), Some(outcome.session_id.as_str()));
        }
    }
    assert_eq!(
        approvals.lock().unwrap().0,
        1,
        "The count grant must be reused only for the same scoped operation"
    );
    assert!(
        approvals.lock().unwrap().1 >= 1,
        "Reading the ungranted file must ask and be denied"
    );
    assert!(events
        .lock()
        .unwrap()
        .iter()
        .any(|(kind, data)| kind == "usage" && data["quota"]["availability"] == "observed"));
    println!("Personal native protocol: 2 completed turns; 1 exact count approval reused; denied file read; same session; observed account quota.");
    crate::capabilities::broker().revoke(&conversation);
    if let Some(session) = native_session {
        let (_cancel, receiver) = watch::channel(false);
        let mut io = transport::ProcessIo::spawn(
            &executable,
            &["app-server".into(), "--listen".into(), "stdio://".into()],
            &ctx,
            receiver,
            control,
            Arc::new(|_, _| {}),
        )
        .unwrap();
        let _=io.request("initialize",json!({"clientInfo":{"name":"coucou","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await;
        let _ = io.send(json!({"method":"initialized","params":{}})).await;
        let _ = io
            .request("thread/archive", json!({"threadId":session}))
            .await;
        io.shutdown().await;
    }
}
