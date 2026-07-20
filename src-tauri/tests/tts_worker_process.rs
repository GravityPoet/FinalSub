use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

struct WorkerHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl WorkerHarness {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_finalsubtauri"))
            .arg("--finalsub-tts-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn isolated TTS worker");
        let stdin = child.stdin.take().expect("worker stdin");
        let stdout = BufReader::new(child.stdout.take().expect("worker stdout")).lines();
        Self {
            child,
            stdin,
            stdout,
        }
    }

    async fn call(&mut self, request: &Value) -> Value {
        let mut encoded = serde_json::to_vec(request).expect("encode worker request");
        encoded.push(b'\n');
        self.call_bytes(&encoded).await
    }

    async fn call_bytes(&mut self, request: &[u8]) -> Value {
        tokio::time::timeout(IO_TIMEOUT, async {
            self.stdin
                .write_all(request)
                .await
                .expect("write worker request");
            self.stdin.flush().await.expect("flush worker request");
            let line = self
                .stdout
                .next_line()
                .await
                .expect("read worker response")
                .expect("worker exited before responding");
            serde_json::from_str(&line).expect("parse worker response")
        })
        .await
        .expect("worker protocol timed out")
    }

    async fn ping(&mut self, request_id: &str) -> Value {
        self.call(&json!({
            "type": "ping",
            "protocol_version": 1,
            "request_id": request_id,
        }))
        .await
    }

    async fn dispose(mut self) {
        let response = self
            .call(&json!({
                "type": "dispose",
                "protocol_version": 1,
                "request_id": "dispose",
            }))
            .await;
        assert_eq!(response["ok"], true);
        let status = tokio::time::timeout(IO_TIMEOUT, self.child.wait())
            .await
            .expect("worker exit timed out")
            .expect("wait for worker");
        assert!(status.success());
    }
}

#[tokio::test]
async fn worker_runs_out_of_process_and_recovers_from_bad_messages() {
    let mut worker = WorkerHarness::spawn();
    let response = worker.ping("first-ping").await;
    assert_eq!(response["ok"], true);
    assert_eq!(response["protocol_version"], 1);
    assert_ne!(
        response["worker_pid"].as_u64(),
        Some(std::process::id() as u64)
    );

    let malformed = worker.call_bytes(b"not-json\n").await;
    assert_eq!(malformed["ok"], false);
    assert_eq!(malformed["request_id"], "invalid");

    let recovered = worker.ping("after-malformed").await;
    assert_eq!(recovered["ok"], true);
    worker.dispose().await;
}

#[tokio::test]
async fn killed_worker_does_not_kill_parent_and_a_new_worker_starts() {
    let mut worker = WorkerHarness::spawn();
    assert_eq!(worker.ping("before-kill").await["ok"], true);
    worker.child.kill().await.expect("kill isolated worker");
    let status = worker.child.wait().await.expect("wait after kill");
    assert!(!status.success());

    let mut replacement = WorkerHarness::spawn();
    assert_eq!(replacement.ping("replacement").await["ok"], true);
    replacement.dispose().await;
}
