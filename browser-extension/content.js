// RDM content script: find media on the page, overlay a dismissible download
// badge on each video, and answer the toolbar popup with what we have seen.

const api = globalThis.browser ?? globalThis.chrome;
const BADGE_CLASS = "rdm-badge";
const found = new Set();

/// Whether badges are shown; persisted so dismissing one hides them everywhere.
let badgesEnabled = true;

function storageGet(key, fallback, cb) {
  try {
    const maybe = api.storage.local.get({ [key]: fallback }, (v) => cb(v[key]));
    if (maybe && typeof maybe.then === "function") {
      maybe.then((v) => cb(v[key]), () => cb(fallback));
    }
  } catch (_) {
    cb(fallback);
  }
}

function storageSet(key, value) {
  try {
    api.storage.local.set({ [key]: value });
  } catch (_) {
    /* ignore */
  }
}

storageGet("badges", true, (enabled) => {
  badgesEnabled = enabled !== false;
  if (!badgesEnabled) removeBadges();
});

try {
  api.storage.onChanged.addListener((changes, area) => {
    if (area !== "local" || !changes.badges) return;
    badgesEnabled = changes.badges.newValue !== false;
    if (badgesEnabled) scan();
    else removeBadges();
  });
} catch (_) {
  /* storage may be unavailable */
}

function record(url) {
  if (url && /^https?:/i.test(url) && !found.has(url)) {
    found.add(url);
  }
}

function mediaUrl(element) {
  const source = element.querySelector && element.querySelector("source");
  return element.currentSrc || element.src || (source && source.src) || "";
}

/// What to hand RDM for a given media element: the direct file when we have a
/// real http(s) URL, otherwise the page itself (yt-dlp resolves `blob:` players).
function targetUrl(element) {
  const media = mediaUrl(element);
  return /^https?:/i.test(media) ? media : location.href;
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

function removeBadges() {
  document.querySelectorAll("." + BADGE_CLASS).forEach((el) => el.remove());
}

/// Hide the badges everywhere, and remember it so they stay hidden.
function dismissBadges() {
  badgesEnabled = false;
  removeBadges();
  storageSet("badges", false);
}

function addBadge(element) {
  if (!badgesEnabled || element.dataset.rdmBadged) return;
  element.dataset.rdmBadged = "1";

  const wrap = document.createElement("div");
  wrap.className = BADGE_CLASS;

  const go = document.createElement("button");
  go.type = "button";
  go.className = "rdm-badge-go";
  go.textContent = "⬇ Download using RDM";
  go.title = "Download using RDM";
  go.addEventListener(
    "click",
    (event) => {
      event.preventDefault();
      event.stopPropagation();
      request(targetUrl(element));
    },
    true,
  );

  const close = document.createElement("button");
  close.type = "button";
  close.className = "rdm-badge-close";
  close.textContent = "×";
  close.title = "Hide these buttons";
  close.addEventListener(
    "click",
    (event) => {
      event.preventDefault();
      event.stopPropagation();
      dismissBadges();
    },
    true,
  );

  wrap.appendChild(go);
  wrap.appendChild(close);

  const parent = element.parentElement;
  if (!parent) return;
  if (getComputedStyle(parent).position === "static") {
    parent.style.position = "relative";
  }
  parent.appendChild(wrap);
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
    respond({ media: Array.from(found), badges: badgesEnabled });
    return true;
  }
  return undefined;
});
