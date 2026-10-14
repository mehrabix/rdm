//! End-to-end test of the IPC path: a request over the socket is turned into a
//! real download by the manager, against a tiny in-process `Range` server.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rdm_core::ipc::{self, Request, ServeOptions};
use rdm_core::{DownloadManager, DownloadSpec, DownloadStatus, EngineConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Minimal HTTP/1.1 server serving `data`, honouring `Range` when asked.
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

#[cfg(unix)]
#[tokio::test]
async fn add_over_the_socket_downloads_the_file() {
    let data = payload(8192);
    let addr = spawn_server(data.clone(), true).await;

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rdm.sock");
    let manager = DownloadManager::new(EngineConfig::default()).unwrap();

    let serve_manager = manager.clone();
    let serve_socket = socket.clone();
    let opts = ServeOptions {
        download_dir: dir.path().to_path_buf(),
        organize_by_category: false,
        connections: 4,
        queue: "Main".into(),
        gate: None,
    };
    tokio::spawn(async move {
        let _ = ipc::serve(serve_manager, serve_socket, opts).await;
    });

    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let response = ipc::request(
        &socket,
        &Request::Add {
            url: format!("http://{addr}/file.bin"),
            file_name: None,
            referer: None,
            cookies: None,
            user_agent: None,
            connections: Some(4),
            queue: None,
            height: None,
            content_type: None,
        },
    )
    .await
    .unwrap();

    assert!(response.ok, "unexpected error: {:?}", response.error);
    let id = response.id.unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let task = manager.get(&id).await.expect("task exists");
        if task.status.is_terminal() {
            assert_eq!(task.status, DownloadStatus::Completed, "{:?}", task.error);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out; last status = {:?}",
            task.status
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let written = std::fs::read(dir.path().join("file.bin")).unwrap();
    assert_eq!(written, data);
}

/// A gated server offers the download to the app instead of queueing it, so the
/// UI can confirm it (IDM's "Download File Info") before anything transfers.
#[cfg(unix)]
#[tokio::test]
async fn a_gated_add_is_offered_not_queued() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("rdm.sock");
    let manager = DownloadManager::new(EngineConfig::default()).unwrap();
    let (gate, mut offered) = tokio::sync::mpsc::unbounded_channel::<DownloadSpec>();

    let opts = ServeOptions {
        download_dir: dir.path().to_path_buf(),
        organize_by_category: false,
        connections: 4,
        queue: "Main".into(),
        gate: Some(gate),
    };
    let serve_socket = socket.clone();
    let serve_manager = manager.clone();
    tokio::spawn(async move {
        let _ = ipc::serve(serve_manager, serve_socket, opts).await;
    });

    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let response = ipc::request(
        &socket,
        &Request::Add {
            url: "https://example.test/clip.mp4".into(),
            file_name: None,
            referer: Some("https://example.test/page".into()),
            cookies: None,
            user_agent: None,
            connections: None,
            queue: None,
            height: None,
            content_type: Some("video/mp4".into()),
        },
    )
    .await
    .unwrap();

    assert!(response.ok, "{response:?}");
    assert_eq!(response.id, None, "an offered download has no id yet");

    let spec = tokio::time::timeout(Duration::from_secs(2), offered.recv())
        .await
        .expect("the app should have been offered the download")
        .expect("the gate must stay open");
    assert_eq!(spec.url, "https://example.test/clip.mp4");
    assert_eq!(spec.content_type.as_deref(), Some("video/mp4"));
    assert_eq!(spec.referer.as_deref(), Some("https://example.test/page"));

    // Offered, not queued: the engine never saw it.
    assert!(manager.list().await.is_empty());
}
