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
    async fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_finalsubtauri"))
            .arg("--finalsub-parakeet-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn isolated Parakeet worker");
        let stdin = child.stdin.take().expect("worker stdin");
        let stdout = BufReader::new(child.stdout.take().expect("worker stdout")).lines();
        let mut harness = Self {
            child,
            stdin,
            stdout,
        };
        let ready = harness.next_event(IO_TIMEOUT).await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["protocol_version"], 1);
        assert_ne!(
            ready["worker_pid"].as_u64(),
            Some(std::process::id() as u64)
        );
        harness
    }

    async fn next_event(&mut self, timeout: Duration) -> Value {
        let line = tokio::time::timeout(timeout, self.stdout.next_line())
            .await
            .expect("worker protocol timed out")
            .expect("read worker response")
            .expect("worker exited before responding");
        serde_json::from_str(&line).expect("parse worker response")
    }

    async fn send(&mut self, request: &Value) {
        let mut encoded = serde_json::to_vec(request).expect("encode worker request");
        encoded.push(b'\n');
        self.stdin
            .write_all(&encoded)
            .await
            .expect("write worker request");
        self.stdin.flush().await.expect("flush worker request");
    }
}

#[tokio::test]
async fn worker_runs_out_of_process_and_rejects_malformed_requests() {
    let mut worker = WorkerHarness::spawn().await;
    worker
        .stdin
        .write_all(b"not-json\n")
        .await
        .expect("write malformed request");
    worker.stdin.flush().await.expect("flush malformed request");
    let response = worker.next_event(IO_TIMEOUT).await;
    assert_eq!(response["type"], "error");
    assert_eq!(response["request_id"], "invalid");
    let status = tokio::time::timeout(IO_TIMEOUT, worker.child.wait())
        .await
        .expect("worker exit timed out")
        .expect("wait for worker");
    assert!(status.success());
}

#[tokio::test]
async fn killing_worker_does_not_kill_parent_test_process() {
    let mut worker = WorkerHarness::spawn().await;
    worker.child.kill().await.expect("kill worker");
    let status = worker.child.wait().await.expect("wait for killed worker");
    assert!(!status.success());

    let mut replacement = WorkerHarness::spawn().await;
    replacement
        .send(&json!({
            "protocol_version": 1,
            "request_id": "invalid-paths",
            "model_dir": "relative",
            "vad_model_path": "relative",
            "audio_path": "relative.wav",
            "max_subtitle_chars": 0,
        }))
        .await;
    let response = replacement.next_event(IO_TIMEOUT).await;
    assert_eq!(response["type"], "error");
    assert_eq!(response["request_id"], "invalid-paths");
}

#[tokio::test]
#[ignore = "requires FINALSUB_PARAKEET_MODEL_DIR, FINALSUB_PARAKEET_LONG_WAV and FINALSUB_SHERPA_VAD_MODEL"]
async fn real_long_audio_is_segmented_and_transcribed_without_aborting() {
    let model_dir = std::env::var("FINALSUB_PARAKEET_MODEL_DIR")
        .expect("FINALSUB_PARAKEET_MODEL_DIR is required");
    let audio_path = std::env::var("FINALSUB_PARAKEET_LONG_WAV")
        .expect("FINALSUB_PARAKEET_LONG_WAV is required");
    let vad_model_path =
        std::env::var("FINALSUB_SHERPA_VAD_MODEL").expect("FINALSUB_SHERPA_VAD_MODEL is required");

    let mut worker = WorkerHarness::spawn().await;
    worker
        .send(&json!({
            "protocol_version": 1,
            "request_id": "long-audio",
            "model_dir": model_dir,
            "vad_model_path": vad_model_path,
            "audio_path": audio_path,
            "max_subtitle_chars": 0,
        }))
        .await;

    let mut progress_events = 0usize;
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let event = worker.next_event(Duration::from_secs(180)).await;
            match event["type"].as_str() {
                Some("progress") => progress_events += 1,
                Some("result") => break event,
                Some("error") => panic!("worker returned error: {}", event["error"]),
                other => panic!("unexpected worker event: {other:?}"),
            }
        }
    })
    .await
    .expect("long audio transcription timed out");

    let cues = result["track"]["cues"]
        .as_array()
        .expect("result cues array");
    assert!(progress_events >= 3);
    assert!(cues.len() > 1);
    assert!(cues.windows(2).all(|pair| {
        pair[0]["start_ms"].as_u64().unwrap() <= pair[1]["start_ms"].as_u64().unwrap()
    }));
    assert!(
        cues.last().unwrap()["end_ms"].as_u64().unwrap() > 60_000,
        "the real long file should produce timestamps beyond the first minute"
    );
    let status = worker.child.wait().await.expect("wait for worker");
    assert!(status.success());
}
