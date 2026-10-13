//! Local IPC: a line-delimited JSON protocol over a Unix domain socket so an
//! external process — the browser native-messaging host — can hand URLs to a
//! running RDM instance.
//!
//! One request per line; the server answers each with one JSON line. The
//! transport is deliberately tiny; the native host is the only intended client.
//!
//! The socket lives in a per-user directory (`$XDG_RUNTIME_DIR`, else
//! `~/.local/share/rdm`), so only the same user can reach it. Requests are
//! validated and never executed as shell commands.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::DownloadManager;
use crate::error::{Error, Result};
use crate::types::DownloadSpec;

/// A request from a client to the running app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    /// Liveness check, so the host can detect a running app.
    Ping,
    /// Queue a download.
    Add {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        referer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cookies: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user_agent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        connections: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queue: Option<String>,
    },
}

/// The reply to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: Option<String>) -> Self {
        Self {
            ok: true,
            id,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            id: None,
            error: Some(message.into()),
        }
    }
}

/// Default socket path: `$XDG_RUNTIME_DIR/rdm.sock`, else
/// `~/.local/share/rdm/rdm.sock`.
pub fn default_socket_path() -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
    {
        return runtime.join("rdm.sock");
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"));
    base.join("rdm").join("rdm.sock")
}

/// Defaults the app injects so the engine's user preferences are not hard-coded
/// in the core.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub download_dir: PathBuf,
    pub organize_by_category: bool,
    pub connections: u8,
    pub queue: String,
}

#[allow(clippy::too_many_arguments)]
async fn add(
    manager: &Arc<DownloadManager>,
    opts: &ServeOptions,
    url: String,
    file_name: Option<String>,
    referer: Option<String>,
    cookies: Option<String>,
    user_agent: Option<String>,
    connections: Option<u8>,
    queue: Option<String>,
) -> Response {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Response::error(format!("unsupported URL (only http/https): {url}"));
    }

    let mut spec =
        DownloadSpec::new(url, opts.download_dir.clone()).connections(connections.unwrap_or(opts.connections).max(1));
    spec.file_name = file_name.filter(|name| !name.trim().is_empty());
    spec.referer = referer;
    spec.cookies = cookies;
    spec.user_agent = user_agent;
    spec.queue = queue
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| opts.queue.clone());
    // The engine decides the per-category folder after it probes.
    spec.organize = opts.organize_by_category;

    match manager.add(spec).await {
        Ok(id) => Response::ok(Some(id)),
        Err(err) => Response::error(err.to_string()),
    }
}

async fn dispatch(manager: &Arc<DownloadManager>, opts: &ServeOptions, req: Request) -> Response {
    match req {
        Request::Ping => Response::ok(None),
        Request::Add {
            url,
            file_name,
            referer,
            cookies,
            user_agent,
            connections,
            queue,
        } => {
            add(
                manager, opts, url, file_name, referer, cookies, user_agent, connections, queue,
            )
            .await
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{UnixListener, UnixStream};

    /// Serve requests until the process exits. Removes a stale socket left by a
    /// crashed run, but refuses to steal one a live app already owns.
    pub async fn serve(
        manager: Arc<DownloadManager>,
        path: PathBuf,
        opts: ServeOptions,
    ) -> Result<()> {
        if path.exists() {
            match UnixStream::connect(&path).await {
                Ok(_) => return Err(Error::invalid("another RDM instance is already listening")),
                Err(_) => {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let listener = UnixListener::bind(&path)?;
        loop {
            let (stream, _) = listener.accept().await?;
            let manager = manager.clone();
            let opts = opts.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_connection(stream, manager, opts).await {
                    tracing::debug!(%err, "ipc connection ended");
                }
            });
        }
    }

    async fn handle_connection(
        stream: UnixStream,
        manager: Arc<DownloadManager>,
        opts: ServeOptions,
    ) -> Result<()> {
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        while let Some(line) = lines.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let response = match serde_json::from_str::<Request>(line) {
                Ok(req) => dispatch(&manager, &opts, req).await,
                Err(err) => Response::error(format!("bad request: {err}")),
            };
            let mut out = serde_json::to_string(&response)?;
            out.push('\n');
            write.write_all(out.as_bytes()).await?;
            write.flush().await?;
        }
        Ok(())
    }

    /// Send one request and read the reply.
    pub async fn request(path: &Path, req: &Request) -> Result<Response> {
        let stream = UnixStream::connect(path).await?;
        let (read, mut write) = stream.into_split();
        let mut out = serde_json::to_string(req)?;
        out.push('\n');
        write.write_all(out.as_bytes()).await?;
        write.flush().await?;

        let mut lines = BufReader::new(read).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| Error::internal("no response from RDM"))?;
        Ok(serde_json::from_str(&line)?)
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;

    const UNSUPPORTED: &str = "browser integration is not supported on Windows yet";

    pub async fn serve(
        _manager: Arc<DownloadManager>,
        _path: PathBuf,
        _opts: ServeOptions,
    ) -> Result<()> {
        Err(Error::invalid(UNSUPPORTED))
    }

    pub async fn request(_path: &Path, _req: &Request) -> Result<Response> {
        Err(Error::invalid(UNSUPPORTED))
    }
}

pub use platform::{request, serve};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineConfig;
    use std::time::Duration;

    #[test]
    fn request_uses_an_action_tag() {
        assert_eq!(serde_json::to_string(&Request::Ping).unwrap(), r#"{"action":"ping"}"#);
        let req: Request =
            serde_json::from_str(r#"{"action":"add","url":"https://example.test/f.zip"}"#).unwrap();
        assert_eq!(
            req,
            Request::Add {
                url: "https://example.test/f.zip".into(),
                file_name: None,
                referer: None,
                cookies: None,
                user_agent: None,
                connections: None,
                queue: None,
            }
        );
    }

    #[test]
    fn response_omits_absent_fields() {
        assert_eq!(
            serde_json::to_string(&Response::ok(Some("id1".into()))).unwrap(),
            r#"{"ok":true,"id":"id1"}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::error("boom")).unwrap(),
            r#"{"ok":false,"error":"boom"}"#
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ping_and_reject_non_http_over_a_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("rdm.sock");
        let manager = DownloadManager::new(EngineConfig::default()).unwrap();
        let opts = ServeOptions {
            download_dir: dir.path().to_path_buf(),
            organize_by_category: false,
            connections: 4,
            queue: "Main".into(),
        };
        let serve_path = socket.clone();
        tokio::spawn(async move {
            let _ = serve(manager, serve_path, opts).await;
        });

        for _ in 0..100 {
            if socket.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert!(request(&socket, &Request::Ping).await.unwrap().ok);

        let rejected = request(
            &socket,
            &Request::Add {
                url: "file:///etc/passwd".into(),
                file_name: None,
                referer: None,
                cookies: None,
                user_agent: None,
                connections: None,
                queue: None,
            },
        )
        .await
        .unwrap();
        assert!(!rejected.ok);
        assert!(rejected.error.unwrap().contains("http"));
    }
}
