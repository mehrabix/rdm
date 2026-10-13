//! End-to-end site-grabber tests against a tiny in-process site, so the crawl
//! (depth, filters, classification) is exercised over real HTTP.

use std::collections::HashMap;
use std::sync::Arc;

use rdm_core::{DownloadManager, EngineConfig, GrabOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Serve a fixed route table: path -> (content type, body). Anything else 404s.
async fn spawn_site(routes: HashMap<&'static str, (&'static str, &'static str)>) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let routes = Arc::new(routes);

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                continue;
            };
            let routes = routes.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 2048];
                loop {
                    match socket.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return,
                    }
                }

                let request = String::from_utf8_lossy(&buf).to_string();
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();

                let response = match routes.get(path.as_str()) {
                    Some((content_type, body)) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                };
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    addr
}

fn site() -> HashMap<&'static str, (&'static str, &'static str)> {
    HashMap::from([
        (
            "/index.html",
            (
                "text/html",
                r#"<html><body>
                     <a href="/a.mp4">a</a>
                     <a href="/ad.mp4">ad</a>
                     <a href="/pack.zip">pack</a>
                     <a href="/sub/page.html">more</a>
                     <a href="https://elsewhere.test/x.mp4">offsite media</a>
                   </body></html>"#,
            ),
        ),
        ("/sub/page.html", ("text/html", r#"<video src="/deep.mp4"></video>"#)),
        ("/a.mp4", ("video/mp4", "aaaa")),
        ("/ad.mp4", ("video/mp4", "adad")),
        ("/deep.mp4", ("video/mp4", "deep")),
        ("/pack.zip", ("application/zip", "zip")),
    ])
}

async fn grab(addr: &std::net::SocketAddr, options: GrabOptions) -> Vec<String> {
    let manager = DownloadManager::new(EngineConfig::default()).unwrap();
    let found = manager
        .grab_site(&format!("http://{addr}/index.html"), options)
        .await
        .unwrap();
    assert!(found.iter().all(|item| item.kind.is_downloadable()));
    found.into_iter().map(|item| item.url).collect()
}

#[tokio::test]
async fn grabs_the_start_page_and_classifies_what_it_links_to() {
    let addr = spawn_site(site()).await;
    let urls = grab(&addr, GrabOptions { depth: 0, ..GrabOptions::default() }).await;

    assert!(urls.contains(&format!("http://{addr}/a.mp4")), "{urls:?}");
    assert!(urls.contains(&format!("http://{addr}/pack.zip")), "{urls:?}");
    // The sub-page is a link to crawl, not something to download.
    assert!(!urls.iter().any(|url| url.ends_with("/sub/page.html")));
    // Depth 0 does not follow it.
    assert!(!urls.iter().any(|url| url.ends_with("/deep.mp4")));
}

#[tokio::test]
async fn follows_links_to_the_requested_depth() {
    let addr = spawn_site(site()).await;
    let urls = grab(&addr, GrabOptions { depth: 1, ..GrabOptions::default() }).await;
    assert!(urls.iter().any(|url| url.ends_with("/deep.mp4")), "{urls:?}");
}

#[tokio::test]
async fn filters_include_and_exclude_globs() {
    let addr = spawn_site(site()).await;
    let urls = grab(
        &addr,
        GrabOptions {
            depth: 1,
            include: vec!["*.mp4".into()],
            exclude: vec!["*/ad.*".into()],
            ..GrabOptions::default()
        },
    )
    .await;

    assert!(urls.iter().any(|url| url.ends_with("/a.mp4")), "{urls:?}");
    assert!(urls.iter().any(|url| url.ends_with("/deep.mp4")), "{urls:?}");
    assert!(!urls.iter().any(|url| url.contains("/ad.mp4")), "{urls:?}");
    assert!(!urls.iter().any(|url| url.ends_with(".zip")), "{urls:?}");
}

#[tokio::test]
async fn stays_on_the_start_origin_by_default() {
    let addr = spawn_site(site()).await;

    // On by default: nothing from another host is collected or crawled.
    let urls = grab(&addr, GrabOptions { depth: 1, ..GrabOptions::default() }).await;
    assert!(!urls.iter().any(|url| url.contains("elsewhere.test")), "{urls:?}");

    // Turned off, off-site media comes along (that is where CDNs live).
    let urls = grab(
        &addr,
        GrabOptions { depth: 1, same_origin: false, ..GrabOptions::default() },
    )
    .await;
    assert!(urls.iter().any(|url| url.starts_with("https://elsewhere.test")), "{urls:?}");
}

#[tokio::test]
async fn honours_the_result_limit() {
    let addr = spawn_site(site()).await;
    let urls = grab(&addr, GrabOptions { depth: 1, limit: 2, ..GrabOptions::default() }).await;
    assert_eq!(urls.len(), 2);
}

#[tokio::test]
async fn rejects_a_non_http_url() {
    let manager = DownloadManager::new(EngineConfig::default()).unwrap();
    let result = manager
        .grab_site("ftp://example.test/file", GrabOptions::default())
        .await;
    assert!(result.is_err());
}

