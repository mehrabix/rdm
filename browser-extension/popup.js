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

ping();
loadMedia();
