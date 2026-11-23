# WorkScene 浏览器标签页扩展

可选的 Chrome / Edge 扩展：读取当前浏览器窗口和标签页 URL，通过 Native Messaging
同步给本机 WorkScene。扩展不发起网络请求，不读取 Cookie、密码、历史记录或页面内容。

## 安装

1. 打开 WorkScene → 设置 → 浏览器标签页 → **安装本机连接**。
2. 点 **打开扩展目录**。
3. Chrome 打开 `chrome://extensions`，Edge 打开 `edge://extensions`。
4. 开启右上角「开发者模式」。
5. 点「加载已解压的扩展程序」，选择本目录（包含 `manifest.json` 的目录）。
6. 扩展应显示为「WorkScene 浏览器标签页」，ID 固定为
   `pijcngaddijnocfoikelinkigoacigbl`。
7. 切回 WorkScene，点「重新检查」；状态应显示已安装/最近同步时间。

## 说明

- 浏览器关闭时 WorkScene 无法知道标签页 URL，只能打开一个空白窗口；
  建议恢复前保持浏览器窗口打开。
- 只同步 `http(s)://` 和 `file://` 页面。
- 恢复时按窗口一次性传入 URL，例如：
  `msedge.exe --new-window url1 url2 url3`，标签页不会被拆成多个窗口。
- 卸载扩展或删除本机连接不会删除已经保存的工作空间。
