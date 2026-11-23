const HOST = "com.workscene.browser_tabs";
const BROWSER = /\bEdg\//.test(navigator.userAgent) ? "edge" : "chrome";
let timer = null;
let lastPayload = "";

function setStatus(ok, message, tabs = 0) {
  chrome.storage.local.set({
    bridgeStatus: { ok, message, tabs, at: Date.now() },
  });
}

function collect(force = false) {
  chrome.windows.getAll({ populate: true, windowTypes: ["normal"] }, (windows) => {
    if (chrome.runtime.lastError) {
      setStatus(false, chrome.runtime.lastError.message);
      return;
    }
    const payload = {
      protocol: 1,
      browser: BROWSER,
      capturedAt: Date.now(),
      windows: (windows || [])
        .filter((w) => !w.incognito)
        .map((w) => ({
          id: w.id,
          focused: !!w.focused,
          state: w.state || "normal",
          tabs: (w.tabs || [])
            .filter((t) => /^(https?|file):/i.test(t.url || ""))
            .map((t) => ({
              title: t.title || "",
              url: t.url,
              active: !!t.active,
            })),
        }))
        .filter((w) => w.tabs.length > 0),
    };
    const json = JSON.stringify(payload);
    if (!force && json === lastPayload) return;
    lastPayload = json;
    chrome.runtime.sendNativeMessage(HOST, payload, (response) => {
      if (chrome.runtime.lastError) {
        setStatus(false, "本机连接未安装或不可用：" + chrome.runtime.lastError.message);
        return;
      }
      if (!response || !response.ok) {
        setStatus(false, (response && response.error) || "WorkScene 未返回成功");
        return;
      }
      setStatus(true, "已同步", response.tabs || 0);
    });
  });
}

function schedule() {
  clearTimeout(timer);
  timer = setTimeout(() => collect(false), 700);
}

chrome.runtime.onStartup.addListener(schedule);
chrome.runtime.onInstalled.addListener(schedule);
chrome.tabs.onUpdated.addListener(() => schedule());
chrome.tabs.onActivated.addListener(() => schedule());
chrome.tabs.onRemoved.addListener(() => schedule());
chrome.windows.onFocusChanged.addListener(() => schedule());
chrome.windows.onCreated.addListener(() => schedule());
chrome.windows.onRemoved.addListener(() => schedule());

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message && message.type === "sync") {
    collect(true);
    sendResponse({ ok: true });
  }
});

schedule();
