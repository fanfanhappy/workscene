const statusEl = document.getElementById("status");
const syncBtn = document.getElementById("sync");

function render(status) {
  if (!status) {
    statusEl.className = "";
    statusEl.textContent = "尚未同步";
    return;
  }
  statusEl.className = status.ok ? "ok" : "err";
  const time = new Date(status.at || Date.now()).toLocaleTimeString();
  statusEl.textContent = `${status.message || "已同步"}（${status.tabs || 0} 个标签页，${time}）`;
}

chrome.storage.local.get("bridgeStatus", (value) => render(value.bridgeStatus));
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.bridgeStatus) render(changes.bridgeStatus.newValue);
});

syncBtn.addEventListener("click", () => {
  syncBtn.disabled = true;
  chrome.runtime.sendMessage({ type: "sync" }, () => {
    setTimeout(() => {
      chrome.storage.local.get("bridgeStatus", (value) => {
        render(value.bridgeStatus);
        syncBtn.disabled = false;
      });
    }, 900);
  });
});
