//! Thin HTTP layer: range probing and ranged fetches.

use std::time::Duration;

use reqwest::header::{self, HeaderMap};
use reqwest::{Client, Response};
use tracing::debug;

use crate::error::{Error, Result};
use crate::types::RemoteInfo;
use crate::util;

const DEFAULT_USER_AGENT: &str = concat!("rdm/", env!("CARGO_PKG_VERSION"));

/// Per-request knobs mirroring the fields a browser would send.
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub cookies: Option<String>,
}

impl RequestOptions {
    pub fn from_spec(spec: &crate::types::DownloadSpec) -> Self {
        Self {
            user_agent: spec.user_agent.clone(),
            referer: spec.referer.clone(),
            cookies: spec.cookies.clone(),
        }
    }
}

/// Build the shared connection-pooling client used for every transfer.
pub fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .pool_max_idle_per_host(32)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?)
}

fn with_headers(req: reqwest::RequestBuilder, opts: &RequestOptions) -> reqwest::RequestBuilder {
    let mut req = req;
    if let Some(ua) = &opts.user_agent {
        req = req.header(header::USER_AGENT, ua);
    }
    if let Some(referer) = &opts.referer {
        req = req.header(header::REFERER, referer);
    }
    if let Some(cookies) = &opts.cookies {
        req = req.header(header::COOKIE, cookies);
    }
    req
}

/// Ask the server what it knows about a URL: size, range support, file name,
/// validators. Uses `Range: bytes=0-0` so we learn both in a single round trip.
pub async fn probe(client: &Client, url: &str, opts: &RequestOptions) -> Result<RemoteInfo> {
    let resp = with_headers(client.get(url).header(header::RANGE, "bytes=0-0"), opts)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT
        || resp.headers().contains_key(header::ACCEPT_RANGES)
        || resp.headers().contains_key(header::CONTENT_RANGE)
    {
        return Ok(from_response(url, &resp, true));
    }

    if resp.status().is_success() {
        return Ok(from_response(url, &resp, false));
    }

    // Some servers reject ranged GETs outright; fall back to HEAD.
    debug!(%url, status = %resp.status(), "ranged probe rejected, retrying with HEAD");
    let head = with_headers(client.head(url), opts).send().await?;
    if !head.status().is_success() {
        return Err(Error::invalid(format!(
            "server returned {} for {url}",
            head.status()
        )));
    }
    Ok(from_response(url, &head, false))
}

fn from_response(url: &str, resp: &Response, ranged: bool) -> RemoteInfo {
    let headers = resp.headers();
    let final_url = resp.url().to_string();

    let total_size = headers
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_range_total)
        .or_else(|| {
            headers
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        });

    let accept_ranges = ranged
        || headers
            .get(header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("bytes"))
            .unwrap_or(false);

    let file_name = content_disposition_name(headers)
        .map(|n| util::sanitize_filename(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| util::filename_from_url(&final_url));

    RemoteInfo {
        url: url.to_string(),
        final_url,
        file_name,
        content_type: header_string(headers, header::CONTENT_TYPE),
        total_size,
        accept_ranges,
        etag: header_string(headers, header::ETAG),
        last_modified: header_string(headers, header::LAST_MODIFIED),
    }
}

/// Fetch an inclusive byte range. Accepts `206` (real range) and `200`
/// (server ignored the range and is sending the whole body).
pub async fn fetch_range(
    client: &Client,
    url: &str,
    start: u64,
    end: u64,
    opts: &RequestOptions,
) -> Result<Response> {
    let resp = with_headers(
        client.get(url).header(header::RANGE, format!("bytes={start}-{end}")),
        opts,
    )
    .send()
    .await?;

    match resp.status() {
        reqwest::StatusCode::PARTIAL_CONTENT | reqwest::StatusCode::OK => Ok(resp),
        status => Err(Error::invalid(format!(
            "unexpected status {status} for range {start}-{end}"
        ))),
    }
}

/// Fetch an entire resource as a stream (used when the size is unknown).
pub async fn fetch_full(client: &Client, url: &str, opts: &RequestOptions) -> Result<Response> {
    let resp = with_headers(client.get(url), opts).send().await?;
    if !resp.status().is_success() {
        return Err(Error::invalid(format!(
            "server returned {} for {url}",
            resp.status()
        )));
    }
    Ok(resp)
}

/// Fetch a whole resource as text (HLS/DASH playlists).
pub async fn fetch_text(client: &Client, url: &str, opts: &RequestOptions) -> Result<String> {
    Ok(fetch_full(client, url, opts).await?.text().await?)
}

/// Fetch a whole resource as bytes (HLS/DASH segments).
pub async fn fetch_bytes(client: &Client, url: &str, opts: &RequestOptions) -> Result<Vec<u8>> {
    Ok(fetch_full(client, url, opts).await?.bytes().await?.to_vec())
}

fn header_string(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// `bytes 0-0/12345` -> `Some(12345)`; a `*` total yields `None`.
fn parse_content_range_total(value: &str) -> Option<u64> {
    let total = value.rsplit('/').next()?.trim();
    if total == "*" {
        None
    } else {
        total.parse().ok()
    }
}

/// Extract a file name from a `Content-Disposition` header, honouring both the
/// RFC 5987 `filename*` form and the plain `filename` form.
fn content_disposition_name(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())?;

    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            // e.g. UTF-8''my%20file.zip
            let encoded = rest.rsplit("''").next().unwrap_or(rest);
            return Some(util::percent_decode(encoded.trim_matches('"')));
        }
        if let Some(rest) = part.strip_prefix("filename=") {
            return Some(rest.trim().trim_matches('"').to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_total() {
        assert_eq!(parse_content_range_total("bytes 0-0/12345"), Some(12345));
        assert_eq!(parse_content_range_total("bytes 0-1/*"), None);
        assert_eq!(parse_content_range_total("nonsense"), None);
    }

    #[test]
    fn content_disposition_variants() {
        let mut h = HeaderMap::new();
        h.insert(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"report final.pdf\"".parse().unwrap(),
        );
        assert_eq!(
            content_disposition_name(&h),
            Some("report final.pdf".to_string())
        );

        h.insert(
            header::CONTENT_DISPOSITION,
            "attachment; filename*=UTF-8''my%20file.zip".parse().unwrap(),
        );
        assert_eq!(content_disposition_name(&h), Some("my file.zip".to_string()));
    }
}
