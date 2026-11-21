# 打包、安装与代码签名

## 现状

| 项目 | 状态 |
|---|---|
| 安装包格式 | NSIS（`bundle.targets: ["nsis"]`），当前用户安装（不需要管理员） |
| 安装语言 | 简体中文 / English |
| WebView2 | `downloadBootstrapper`：目标机缺 WebView2 时安装程序自动装 |
| **代码签名** | **未签名**。分发时 Windows SmartScreen 会拦截，需用户点"仍要运行" |
| 自动更新 | 未实现 |

## 构建

```bash
pnpm install
pnpm build                       # 前端类型检查 + 构建
pnpm tauri build                 # 产出安装包
```

产物路径：

```
src-tauri/target/release/bundle/nsis/WorkScene_<version>_x64-setup.exe
```

只验证编译、不打包时用：

```bash
pnpm tauri build --debug --no-bundle
```

## 代码签名（发布前必做）

未签名的安装包在 Windows 上会被 SmartScreen 拦一次，且随着下载量下降信誉会更难绕过。签名需要一张代码签名证书（OV 或 EV），这里给出两种方式：

### 方式一：signtool（推荐，证书在文件或硬件令牌里）

```powershell
# 1) 构建出未签名的安装包
pnpm tauri build

# 2) 签名（时间戳服务器必须带，否则证书到期后签名失效）
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 `
  /f .\cert\workscene.pfx /p $env:CERT_PASSWORD `
  .\src-tauri\target\release\bundle\nsis\WorkScene_0.1.0_x64-setup.exe

# 3) 验签
signtool verify /pa /v .\src-tauri\target\release\bundle\nsis\WorkScene_0.1.0_x64-setup.exe
```

> 主程序 `WorkScene.exe` 也应签名，否则用户从安装目录直接运行时同样被拦。

### 方式二：让 Tauri 在打包时签名

`tauri.conf.json` 的 `bundle.windows` 支持 `signCommand`，例如：

```json
{
  "bundle": {
    "windows": {
      "signCommand": {
        "cmd": "signtool",
        "args": ["sign", "/fd", "SHA256", "/tr", "http://timestamp.digicert.com", "/td", "SHA256", "/f", "./cert/workscene.pfx", "/p", "%1", "%2"]
      }
    }
  }
}
```

（参数占位符以 Tauri 官方文档为准；实际接入前先用方式一跑通一次。）

**不要把证书和密码提交进仓库**：`.gitignore` 已排除 `*.pfx`、`*.p12`。

## 发布前检查清单

- [ ] `cargo test` 全绿
- [ ] `pnpm build` 通过
- [ ] `--m1test` / `--m2test` / `--m5test` 通过
- [ ] `--baseline 5` 成功率 ≥ 95%
- [ ] 在**干净虚拟机**上装一次，确认 WebView2 引导正常
- [ ] 安装包与主程序都已签名并验签
- [ ] 卸载后确认数据目录行为（当前：`%LOCALAPPDATA%\WorkScene` 保留，不随卸载删除）

## 已知缺口

- 没有自动更新（需要 `tauri-plugin-updater` + 更新签名密钥）；
- 没有崩溃转储收集（本地日志以外无遥测，符合"本地优先"）；
- 未做 MSI 目标（NSIS 已覆盖当前需求）。
