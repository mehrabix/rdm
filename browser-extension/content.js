// RDM content script: find media on the page, overlay a download badge on each
// video, and answer the toolbar popup with what we have seen.

const api = globalThis.browser ?? globalThis.chrome;
const BADGE_CLASS = "rdm-badge";
const found = new Set();

function record(url) {
  if (url && /^https?:/i.test(url) && !found.has(url)) {
    found.add(url);
  }
}

function mediaUrl(element) {
  const source = element.querySelector && element.querySelector("source");
  return element.currentSrc || element.src || (source && source.src) || "";
}

function request(url) {
  if (!url) return;
  api.runtime.sendMessage({ type: "rdm:add", url, referer: location.href }, (res) => {
    const err = api.runtime.lastError;
    if (err) {
      console.warn("RDM:", err.message);
      return;
    }
    if (!res || !res.ok) console.warn("RDM:", (res && res.error) || "could not queue");
    else console.info("RDM queued", res.id);
  });
}

function addBadge(element) {
  if (element.dataset.rdmBadged) return;
  element.dataset.rdmBadged = "1";

  const button = document.createElement("button");
  button.type = "button";
  button.className = BADGE_CLASS;
  button.textContent = "⬇ RDM";
  button.title = "Download with RDM";
  button.addEventListener(
    "click",
    (event) => {
      event.preventDefault();
      event.stopPropagation();
      request(mediaUrl(element));
    },
    true,
  );

  const parent = element.parentElement;
  if (!parent) return;
  if (getComputedStyle(parent).position === "static") {
    parent.style.position = "relative";
  }
  parent.appendChild(button);
}

function scan() {
  document.querySelectorAll("video, audio").forEach((element) => {
    record(mediaUrl(element));
    addBadge(element);
  });
  // Players often fetch a manifest/segment directly; surface those too.
  try {
    for (const entry of performance.getEntriesByType("resource")) {
      if (/\.(m3u8|mpd|mp4|m4v|webm|mkv|ts)(\?|#|$)/i.test(entry.name)) {
        record(entry.name);
      }
    }
  } catch (_) {
    /* resource timing may be unavailable */
  }
}

scan();
try {
  new MutationObserver(scan).observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
} catch (_) {
  /* ignore */
}
setInterval(scan, 2500);

api.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message && message.type === "rdm:list") {
    scan();
    respond({ media: Array.from(found) });
    return true;
  }
  return undefined;
});
