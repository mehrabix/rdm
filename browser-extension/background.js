// RDM browser integration.
//
// One file works as a Chromium MV3 service worker and a Firefox event page: the
// `browser`/`chrome` APIs are shimmed, and native messaging is called in a way
// that works with either the callback (Chrome) or Promise (Firefox) convention.

const api = globalThis.browser ?? globalThis.chrome;
const HOST = "io.github.mehrabix.rdm";
const MENU_ID = "rdm-download";

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

/// Forward an http(s) URL to RDM; resolves with `{ok, id|error}`.
async function queue(url, referer) {
  if (!/^https?:/i.test(url || "")) {
    return { ok: false, error: "not an http(s) URL" };
  }
  try {
    const response = await sendNative({ action: "add", url, referer });
    if (!response) return { ok: false, error: "no response from RDM" };
    return response;
  } catch (err) {
    return { ok: false, error: String((err && err.message) || err) };
  }
}

function createMenus() {
  api.contextMenus.removeAll(() => {
    api.contextMenus.create({
      id: MENU_ID,
      title: "Download with RDM",
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
  await report(await queue(url, info.pageUrl), url);
});

api.runtime.onMessage.addListener((message, _sender, respond) => {
  if (!message) return undefined;
  if (message.type === "rdm:add") {
    queue(message.url, message.referer).then(respond);
    return true; // keep the channel open for the async reply
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
