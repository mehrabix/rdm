//! End-to-end engine tests against a tiny in-process HTTP server that speaks
//! `Range` requests, so segmentation and resume logic is exercised for real.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rdm_core::{DownloadManager, DownloadSpec, DownloadStatus, EngineConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Start a one-shot-per-connection HTTP/1.1 server that serves `data` and
/// optionally honours `Range` requests. Returns the bound address.
async fn spawn_server(data: Vec<u8>, supports_ranges: bool) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let shared = Arc::new(data);

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                continue;
            };
            let data = shared.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 2048];
                loop {
                    match socket.read(&mut tmp).await {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&tmp[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return,
                    }
                }

                let request = String::from_utf8_lossy(&buf).to_string();
                let range = request
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("range:").map(str::to_string));

                let total = data.len() as u64;
                let (status, start, end) = match range
                    .as_deref()
                    .filter(|_| supports_ranges)
                    .and_then(parse_range)
                {
                    Some((s, e)) => (206u16, s, e.min(total - 1)),
                    None => (200u16, 0, total - 1),
                };
                let body = &data[start as usize..=end as usize];

                let reason = if status == 206 { "Partial Content" } else { "OK" };
                let mut head = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                if supports_ranges {
                    head.push_str("Accept-Ranges: bytes\r\n");
                }
                if status == 206 {
                    head.push_str(&format!("Content-Range: bytes {start}-{end}/{total}\r\n"));
                }
                head.push_str("\r\n");

                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(body).await;
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
            });
        }
    });

    addr
}

fn parse_range(value: &str) -> Option<(u64, u64)> {
    let value = value.trim().trim_start_matches("bytes=");
    let (start, end) = value.split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = if end.trim().is_empty() {
        u64::MAX
    } else {
        end.trim().parse().ok()?
    };
    Some((start, end))
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

async fn wait_until_terminal(
    mgr: &Arc<DownloadManager>,
    id: &str,
    timeout: Duration,
) -> rdm_core::DownloadTask {
    let deadline = Instant::now() + timeout;
    loop {
        let task = mgr.get(id).await.expect("task exists");
        if task.status.is_terminal() {
            return task;
        }
        assert!(
            Instant::now() < deadline,
            "timed out; last status = {:?}",
            task.status
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn probe_reports_size_and_range_support() {
    let data = payload(4096);
    let addr = spawn_server(data.clone(), true).await;
    let mgr = DownloadManager::new(EngineConfig::default()).unwrap();

    let spec = DownloadSpec::new(format!("http://{addr}/file.bin"), ".");
    let info = mgr.probe(&spec).await.unwrap();

    assert!(info.accept_ranges, "server advertises range support");
    assert_eq!(info.total_size, Some(4096));
    assert_eq!(info.file_name, "file.bin");
}

#[tokio::test]
async fn downloads_file_with_multiple_connections() {
    let data = payload(512 * 1024);
    let addr = spawn_server(data.clone(), true).await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = DownloadManager::new(EngineConfig::default()).unwrap();

    let spec = DownloadSpec::new(format!("http://{addr}/payload.bin"), dir.path())
        .connections(4)
        .file_name("payload.bin");
    let id = mgr.add(spec).await.unwrap();

    let task = wait_until_terminal(&mgr, &id, Duration::from_secs(30)).await;
    assert_eq!(task.status, DownloadStatus::Completed, "error: {:?}", task.error);

    let bytes = std::fs::read(dir.path().join("payload.bin")).unwrap();
    assert_eq!(bytes.len(), data.len(), "file size must match exactly");
    assert_eq!(bytes, data, "file contents must match byte-for-byte");

    // A completed transfer must not leave a resume sidecar behind.
    assert!(!dir.path().join("payload.bin.rdm.json").exists());
}

#[tokio::test]
async fn falls_back_to_single_stream_without_range_support() {
    let data = payload(200 * 1024);
    let addr = spawn_server(data.clone(), false).await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = DownloadManager::new(EngineConfig::default()).unwrap();

    let spec = DownloadSpec::new(format!("http://{addr}/plain.bin"), dir.path()).connections(8);
    let id = mgr.add(spec).await.unwrap();

    let task = wait_until_terminal(&mgr, &id, Duration::from_secs(30)).await;
    assert_eq!(task.status, DownloadStatus::Completed, "error: {:?}", task.error);
    assert_eq!(task.segments.len(), 1, "no range support => one segment");

    let bytes = std::fs::read(dir.path().join("plain.bin")).unwrap();
    assert_eq!(bytes, data);
}

#[tokio::test]
async fn remove_deletes_partial_file() {
    let data = payload(1024);
    let addr = spawn_server(data, true).await;
    let dir = tempfile::tempdir().unwrap();
    let mgr = DownloadManager::new(EngineConfig::default()).unwrap();

    let spec = DownloadSpec::new(format!("http://{addr}/x.bin"), dir.path());
    let id = mgr.add(spec).await.unwrap();
    wait_until_terminal(&mgr, &id, Duration::from_secs(10)).await;

    mgr.remove(&id, true).await.unwrap();
    assert!(mgr.get(&id).await.is_none());
    assert!(!dir.path().join("x.bin").exists());
}
