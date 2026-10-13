// RDM content script: find media on the page, overlay a dismissible download
// badge on each video (with a quality menu), and answer the toolbar popup.

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
    // The background keeps a per-tab registry (fed by both the network monitor
    // and this DOM/MSE scan) that the popup reads.
    try {
      api.runtime.sendMessage({ type: "rdm:sniff", url, referer: location.href });
    } catch (_) {
      /* the service worker may be starting up */
    }
  }
}

// The page-context detector (`main-world.js`) reports what the player actually
// fetched — including traffic that never shows up in the DOM.
window.addEventListener(
  "message",
  (event) => {
    const data = event.data;
    if (!data || data.source !== "rdm-media") return;
    if (data.url) record(data.url);
  },
  false,
);

function mediaUrl(element) {
  const source = element.querySelector && element.querySelector("source");
  return element.currentSrc || element.src || (source && source.src) || "";
}

/// What to hand RDM for a media element: the direct file when we have a real
/// http(s) URL, otherwise the page itself (yt-dlp resolves `blob:` players).
function targetUrl(element) {
  const media = mediaUrl(element);
  return /^https?:/i.test(media) ? media : location.href;
}

/// Queue a download, optionally at a chosen maximum height.
function request(url, height) {
  if (!url) return;
  const message = { type: "rdm:add", url, referer: location.href };
  if (height) message.height = height;
  api.runtime.sendMessage(message, (res) => {
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

// ---- quality menu ----------------------------------------------------------

let openMenuElement = null;

function closeMenus() {
  if (openMenuElement) {
    openMenuElement.remove();
    openMenuElement = null;
  }
  document.removeEventListener("click", onDocumentClick);
  document.removeEventListener("keydown", onKeyDown);
}

function onDocumentClick(event) {
  if (event.target.closest && event.target.closest(".rdm-menu")) return;
  closeMenus();
}

function onKeyDown(event) {
  if (event.key === "Escape") closeMenus();
}

function openMenu(anchor, url) {
  const wasOpen = openMenuElement !== null;
  closeMenus();
  if (wasOpen) return; // clicking the badge again closes it

  const rect = anchor.getBoundingClientRect();
  const menu = document.createElement("div");
  menu.className = "rdm-menu";
  // Anchored to the page, not the badge: no ancestor `overflow` can clip it.
  menu.style.top = `${Math.round(rect.bottom + 4)}px`;
  menu.style.left = `${Math.round(rect.left)}px`;
  menu.style.minWidth = `${Math.round(rect.width)}px`;
  openMenuElement = menu;

  const addItem = (label, height) => {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "rdm-menu-item";
    item.textContent = label;
    item.addEventListener(
      "click",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        closeMenus();
        request(url, height);
      },
      true,
    );
    menu.appendChild(item);
  };

  addItem("Best quality", undefined);
  const note = document.createElement("div");
  note.className = "rdm-menu-note";
  note.textContent = "Loading qualities…";
  menu.appendChild(note);

  document.body.appendChild(menu);

  api.runtime.sendMessage({ type: "rdm:qualities", url, referer: location.href }, (res) => {
    const err = api.runtime.lastError;
    note.remove();
    if (err || !res || !res.ok || !res.heights || res.heights.length === 0) {
      const message = document.createElement("div");
      message.className = "rdm-menu-note";
      message.textContent = err ? "RDM not available" : (res && res.error) || "No other qualities";
      menu.appendChild(message);
      return;
    }
    for (const height of res.heights) addItem(`${height}p`, height);
  });

  // Defer so this same click does not immediately close the menu.
  setTimeout(() => {
    document.addEventListener("click", onDocumentClick);
    document.addEventListener("keydown", onKeyDown);
  }, 0);
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
  go.title = "Download using RDM (click to choose quality)";
  go.addEventListener(
    "click",
    (event) => {
      event.preventDefault();
      event.stopPropagation();
      openMenu(wrap, targetUrl(element));
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
  if (message && message.type === "rdm:dom") {
    scan();
    respond({ media: Array.from(found), badges: badgesEnabled });
    return true;
  }
  return undefined;
});
