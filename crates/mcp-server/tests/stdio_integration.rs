use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[tokio::test]
async fn stdio_transport_handles_initialize_request() {
    let bin = env!("CARGO_BIN_EXE_mcp_stdio");
    let mut child = Command::new(bin)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn stdio binary");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut lines = BufReader::new(stdout).lines();

    let request = serde_json::json!({
        "jsonrpc":"2.0",
        "id":1,
        "method":"initialize",
        "params":{"protocolVersion":"2025-06-18"}
    })
    .to_string();
    stdin
        .write_all(format!("{request}\n").as_bytes())
        .await
        .expect("write request");
    stdin.flush().await.expect("flush");

    let line = lines
        .next_line()
        .await
        .expect("line read")
        .expect("line value");
    let json: serde_json::Value = serde_json::from_str(&line).expect("json response");
    assert_eq!(json["id"], 1);
    assert_eq!(json["result"]["protocolVersion"], "2025-06-18");

    let _ = child.kill().await;
}
