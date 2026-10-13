// RDM browser integration: a "Download with RDM" link context menu that hands
// the URL to the native-messaging host, which forwards it to a running RDM.
//
// One file works as a Chromium MV3 service worker and a Firefox event page: the
// `browser`/`chrome` APIs are shimmed, and native messaging is called in a way
// that works with either the callback (Chrome) or Promise (Firefox) convention.

const api = globalThis.browser ?? globalThis.chrome;
const HOST = "io.github.mehrabix.rdm";
const MENU_ID = "rdm-download";

api.runtime.onInstalled.addListener(() => {
  api.contextMenus.create({
    id: MENU_ID,
    title: "Download with RDM",
    contexts: ["link"],
  });
});

function sendNative(payload) {
  return new Promise((resolve, reject) => {
    try {
      const maybePromise = api.runtime.sendNativeMessage(HOST, payload, (response) => {
        const err = api.runtime.lastError;
        if (err) reject(new Error(err.message));
        else resolve(response);
      });
      // Firefox returns a Promise instead of using the callback.
      if (maybePromise && typeof maybePromise.then === "function") {
        maybePromise.then(resolve, reject);
      }
    } catch (err) {
      reject(err);
    }
  });
}

api.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== MENU_ID || !info.linkUrl) return;
  try {
    const response = await sendNative({
      action: "add",
      url: info.linkUrl,
      referer: info.pageUrl,
    });
    if (response && response.ok) {
      console.info("RDM queued the download", response.id);
    } else {
      console.warn("RDM could not queue the download:", response && response.error);
    }
  } catch (err) {
    console.warn("RDM:", err && err.message ? err.message : err);
  }
});
