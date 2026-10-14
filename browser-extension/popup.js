const api = globalThis.browser ?? globalThis.chrome;

const KIND_LABEL = { hls: "HLS", dash: "DASH", video: "Video", audio: "Audio" };

/// What the page calls itself, so RDM can name the file properly instead of
/// saving `watch.mp4`.
let pageTitle = "";

function setStatus(ok, text) {
  const el = document.getElementById("status");
  el.textContent = text;
  el.className = ok ? "ok" : "bad";
}

function ping() {
  api.runtime.sendMessage({ type: "rdm:ping" }, (res) => {
    const err = api.runtime.lastError;
    if (err) return setStatus(false, "extension error");
    if (res && res.ok) setStatus(true, "connected");
    else setStatus(false, (res && res.error) || "RDM not running");
  });
}

/// The background merges the network registry with the page's DOM/MSE scan.
/// Items are `{url, kind, contentType, referer}`; plain URL strings are still
/// accepted so an older content script cannot break the popup.
function normalize(item) {
  if (typeof item === "string") return { url: item, kind: "video", referer: "" };
  return item || {};
}

function render(items) {
  const list = document.getElementById("media");
  list.textContent = "";
  if (!items.length) {
    const li = document.createElement("li");
    li.className = "empty";
    li.textContent = "No video detected yet — play it, then refresh.";
    list.appendChild(li);
    return;
  }

  for (const raw of items) {
    const item = normalize(raw);
    if (!item.url) continue;

    const li = document.createElement("li");

    const badge = document.createElement("span");
    badge.className = `kind kind-${item.kind || "video"}`;
    badge.textContent = KIND_LABEL[item.kind] || "File";
    li.appendChild(badge);

    const name = document.createElement("span");
    name.className = "name";
    name.textContent = item.url.split("/").pop().split("?")[0] || item.url;
    name.title = item.url;
    li.appendChild(name);

    const button = document.createElement("button");
    button.textContent = "Download";
    button.addEventListener("click", () => {
      api.runtime.sendMessage(
        {
          type: "rdm:add",
          url: item.url,
          referer: item.referer || "",
          contentType: item.contentType || "",
          title: pageTitle,
        },
        () => ping(),
      );
    });
    li.appendChild(button);

    list.appendChild(li);
  }
}

function loadMedia() {
  api.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
    if (!tab) return render([]);
    api.runtime.sendMessage({ type: "rdm:list", tabId: tab.id }, (res) => {
      if (api.runtime.lastError) return render([]);
      pageTitle = (res && res.title) || tab.title || "";
      render((res && res.media) || []);
    });
  });
}

document.getElementById("refresh").addEventListener("click", () => {
  ping();
  loadMedia();
});

// Send the page URL itself: RDM detects the HTML and hands it to yt-dlp, so
// players that fetch the video over a blob: URL still work.
document.getElementById("page").addEventListener("click", () => {
  api.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
    if (!tab || !tab.url) return;
    api.runtime.sendMessage(
      { type: "rdm:add", url: tab.url, referer: "", title: tab.title || "" },
      () => ping(),
    );
  });
});

// Toggle the in-page "Download using RDM" badges (persisted; the badge's own ×
// turns them off, and this turns them back on).
const badges = document.getElementById("badges");
try {
  api.storage.local.get({ badges: true }, (v) => {
    badges.checked = v.badges !== false;
  });
} catch (_) {
  badges.checked = true;
}
badges.addEventListener("change", () => {
  try {
    api.storage.local.set({ badges: badges.checked });
  } catch (_) {
    /* ignore */
  }
});

ping();
loadMedia();
