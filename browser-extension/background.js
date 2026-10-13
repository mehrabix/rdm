// RDM browser integration.
//
// One file works as a Chromium MV3 service worker and a Firefox event page: the
// `browser`/`chrome` APIs are shimmed, and native messaging is called in a way
// that works with either the callback (Chrome) or Promise (Firefox) convention.
//
// This is the "media detector" half of the IDM mechanism: the browser watches
// what the *player* requests (network monitor + `main-world.js` hooks) and keeps
// a per-tab registry of candidates — manifests, direct video, audio — which the
// popup lists and hands to RDM. Detection never downloads by itself.

const api = globalThis.browser ?? globalThis.chrome;
const HOST = "io.github.mehrabix.rdm";
const MENU_ID = "rdm-download";
const MAX_PER_TAB = 60;

/// Call the native host, resolving with its JSON reply.
function sendNative(payload) {
  return new Promise((resolve, reject) => {
    try {
      const maybePromise = api.runtime.sendNativeMessage(HOST, payload, (response) => {
        const err = api.runtime.lastError;
        if (err) reject(new Error(err.message));
        else resolve(response);
      });
      if (maybePromise && typeof maybePromise.then === "function") {
        maybePromise.then(resolve, reject);
      }
    } catch (err) {
      reject(err);
    }
  });
}

/// Read the browser's cookies for `url`, as a `Cookie:` header value. Sites like
/// YouTube require the logged-in session, which yt-dlp then reuses.
function cookiesFor(url) {
  return new Promise((resolve) => {
    if (!api.cookies || !api.cookies.getAll) return resolve("");
    const join = (cookies) =>
      (cookies || []).map((c) => `${c.name}=${c.value}`).join("; ");
    try {
      const maybe = api.cookies.getAll({ url }, (cookies) => {
        if (api.runtime.lastError) return resolve("");
        resolve(join(cookies));
      });
      if (maybe && typeof maybe.then === "function") {
        maybe.then((cookies) => resolve(join(cookies)), () => resolve(""));
      }
    } catch (_) {
      resolve("");
    }
  });
}

// ---- media detection -------------------------------------------------------

/// The per-tab registry: tabId -> Map(url -> candidate).
const mediaByTab = new Map();

const MANIFEST = /\.(m3u8|mpd)(\?|#|$)/i;
const VIDEO_FILE = /\.(mp4|m4v|webm|mkv|mov|avi|flv|ogv)(\?|#|$)/i;
const AUDIO_FILE = /\.(mp3|m4a|aac|flac|wav|ogg|oga|opus)(\?|#|$)/i;
const SEGMENT = /\.(m4s|ts|aac|vtt|cmfv|cmfa)(\?|#|$)/i;

/// Classify a URL by shape. Segments return null: they are pieces of a stream,
/// not something to offer as a download.
function kindForUrl(url) {
  if (typeof url !== "string" || !/^https?:/i.test(url)) return null;
  if (/\.m3u8(\?|#|$)/i.test(url)) return "hls";
  if (/\.mpd(\?|#|$)/i.test(url)) return "dash";
  if (MANIFEST.test(url)) return "hls";
  if (SEGMENT.test(url)) return null;
  if (VIDEO_FILE.test(url)) return "video";
  if (AUDIO_FILE.test(url)) return "audio";
  return null;
}

/// Classify by the media type the server actually sent. This is what catches a
/// player pulling `…/master?token=…` with no useful extension.
function kindForContentType(contentType) {
  const mime = String(contentType || "").split(";")[0].trim().toLowerCase();
  if (!mime) return null;
  if (
    mime === "application/vnd.apple.mpegurl" ||
    mime === "application/x-mpegurl" ||
    mime === "audio/mpegurl" ||
    mime === "audio/x-mpegurl"
  ) {
    return "hls";
  }
  if (mime === "application/dash+xml") return "dash";
  if (mime.startsWith("video/")) return "video";
  if (mime.startsWith("audio/")) return "audio";
  return null;
}

function remember(tabId, url, patch) {
  if (typeof tabId !== "number" || tabId < 0) return;
  if (!/^https?:/i.test(url || "")) return;
  let registry = mediaByTab.get(tabId);
  if (!registry) {
    registry = new Map();
    mediaByTab.set(tabId, registry);
  }
  const existing = registry.get(url) || { url };
  registry.set(url, { ...existing, ...patch, at: Date.now() });
  if (registry.size > MAX_PER_TAB) {
    // Drop the oldest entry; the registry is a hint list, not a log.
    let oldestKey = null;
    let oldestAt = Infinity;
    for (const [key, value] of registry) {
      if (value.at < oldestAt) {
        oldestAt = value.at;
        oldestKey = key;
      }
    }
    if (oldestKey) registry.delete(oldestKey);
  }
}

/// Manifests first, then video, then audio; newest first within a kind.
function rank(candidate) {
  return { hls: 0, dash: 1, video: 2, audio: 3 }[candidate.kind] ?? 4;
}

function mergeMedia(fromRequests, fromDom, referer) {
  const merged = new Map();
  for (const url of fromDom || []) {
    const kind = kindForUrl(url);
    if (!kind) continue;
    merged.set(url, { url, kind, referer });
  }
  for (const candidate of fromRequests || []) {
    if (!candidate || !candidate.url) continue;
    const kind = candidate.kind || kindForUrl(candidate.url);
    if (!kind) continue;
    const existing = merged.get(candidate.url) || {};
    merged.set(candidate.url, {
      ...existing,
      ...candidate,
      kind,
      // The observed request's own referer beats the page URL.
      referer: candidate.referer || existing.referer || referer || "",
    });
  }
  return Array.from(merged.values()).sort((a, b) => rank(a) - rank(b) || b.at - a.at);
}

// Network monitor: watches what the player fetches, including the media type.
if (api.webRequest) {
  try {
    api.webRequest.onBeforeRequest.addListener(
      (details) => {
        const kind = kindForUrl(details.url);
        if (kind) remember(details.tabId, details.url, { kind, referer: details.initiator || "" });
      },
      { urls: ["<all_urls>"], types: ["media", "xmlhttprequest", "object", "other", "sub_frame"] },
    );
  } catch (_) {
    /* webRequest may be unavailable; the DOM scan still runs */
  }

  try {
    api.webRequest.onHeadersReceived.addListener(
      (details) => {
        const headers = details.responseHeaders || [];
        const contentType = headers
          .filter((header) => /^content-type$/i.test(header.name || ""))
          .map((header) => header.value || "")
          .join("");
        const kind = kindForContentType(contentType);
        if (!kind) return;
        remember(details.tabId, details.url, {
          kind,
          contentType,
          referer: details.initiator || "",
        });
      },
      { urls: ["<all_urls>"] },
      ["responseHeaders"],
    );
  } catch (_) {
    /* see above */
  }
}

try {
  api.tabs.onRemoved.addListener((tabId) => mediaByTab.delete(tabId));
} catch (_) {
  /* ignore */
}

// ---- actions ---------------------------------------------------------------

/// Forward an http(s) URL to RDM; resolves with `{ok, id|error}`.
async function queue(url, referer, height, contentType) {
  if (!/^https?:/i.test(url || "")) {
    return { ok: false, error: "not an http(s) URL" };
  }
  try {
    const cookies = await cookiesFor(url);
    const response = await sendNative({
      action: "add",
      url,
      referer,
      cookies,
      height,
      content_type: contentType || "",
    });
    if (!response) return { ok: false, error: "no response from RDM" };
    return response;
  } catch (err) {
    return { ok: false, error: String((err && err.message) || err) };
  }
}

/// Ask RDM which video heights a URL offers.
async function qualities(url, referer) {
  if (!/^https?:/i.test(url || "")) {
    return { ok: false, error: "not an http(s) URL" };
  }
  try {
    // Same cookies as a download: sites that gate downloads gate format lists.
    const cookies = await cookiesFor(url);
    const response = await sendNative({ action: "qualities", url, referer, cookies });
    if (!response) return { ok: false, error: "no response from RDM" };
    return response;
  } catch (err) {
    return { ok: false, error: String((err && err.message) || err) };
  }
}

/// Everything detected for a tab: the network registry merged with the page's
/// own DOM/MSE scan.
function listForTab(tabId, respond) {
  const fromRequests = Array.from((mediaByTab.get(tabId) || new Map()).values());
  api.tabs.sendMessage(tabId, { type: "rdm:dom" }, (res) => {
    const fromDom = api.runtime.lastError ? [] : (res && res.media) || [];
    respond({ ok: true, media: mergeMedia(fromRequests, fromDom, "") });
  });
}

function createMenus() {
  api.contextMenus.removeAll(() => {
    api.contextMenus.create({
      id: MENU_ID,
      title: "Download using RDM",
      contexts: ["link", "video", "audio", "image", "selection"],
    });
  });
}

api.runtime.onInstalled.addListener(createMenus);
api.runtime.onStartup.addListener(createMenus);

api.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== MENU_ID) return;
  const selection = (info.selectionText || "").trim();
  const url =
    info.linkUrl ||
    info.srcUrl ||
    (/^https?:/i.test(selection) ? selection : "");
  const contentType = kindForUrl(url) === "video" ? "video/mp4" : "";
  await report(await queue(url, info.pageUrl, undefined, contentType), url);
});

api.runtime.onMessage.addListener((message, sender, respond) => {
  if (!message) return undefined;
  if (message.type === "rdm:add") {
    queue(message.url, message.referer, message.height, message.contentType).then(respond);
    return true; // keep the channel open for the async reply
  }
  if (message.type === "rdm:qualities") {
    qualities(message.url, message.referer).then(respond);
    return true;
  }
  if (message.type === "rdm:sniff") {
    const tabId = sender && sender.tab ? sender.tab.id : -1;
    const kind = kindForUrl(message.url);
    if (kind) remember(tabId, message.url, { kind, referer: message.referer || "" });
    respond({ ok: true });
    return true;
  }
  if (message.type === "rdm:list") {
    listForTab(message.tabId, respond);
    return true;
  }
  if (message.type === "rdm:ping") {
    sendNative({ action: "ping" })
      .then((res) => respond({ ok: !!(res && res.ok), error: res && res.error }))
      .catch((err) => respond({ ok: false, error: String((err && err.message) || err) }));
    return true;
  }
  return undefined;
});

async function report(result, url) {
  if (result && result.ok) {
    console.info("RDM queued", url, result.id);
    return;
  }
  const message = (result && result.error) || "Could not queue the download";
  console.warn("RDM error:", message);
  try {
    api.notifications.create({
      type: "basic",
      iconUrl: "icons/icon-128.png",
      title: "RDM",
      message,
    });
  } catch (_) {
    /* notifications are best-effort */
  }
}
