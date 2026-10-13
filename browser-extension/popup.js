const api = globalThis.browser ?? globalThis.chrome;

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

function render(media) {
  const list = document.getElementById("media");
  list.textContent = "";
  if (!media.length) {
    const li = document.createElement("li");
    li.className = "empty";
    li.textContent = "No media detected on this page.";
    list.appendChild(li);
    return;
  }
  for (const url of media) {
    const li = document.createElement("li");
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = url.split("/").pop().split("?")[0] || url;
    name.title = url;
    const button = document.createElement("button");
    button.textContent = "Download";
    button.addEventListener("click", () => {
      api.runtime.sendMessage({ type: "rdm:add", url, referer: "" }, () => ping());
    });
    li.appendChild(name);
    li.appendChild(button);
    list.appendChild(li);
  }
}

function loadMedia() {
  api.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
    if (!tab) return render([]);
    api.tabs.sendMessage(tab.id, { type: "rdm:list" }, (res) => {
      if (api.runtime.lastError) return render([]);
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
    api.runtime.sendMessage({ type: "rdm:add", url: tab.url, referer: "" }, () => ping());
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
