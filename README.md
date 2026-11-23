<div align="center">

# WorkScene

**Windows 10 / 11 本地优先桌面工作现场管理器**

将常用应用、窗口布局、多显示器拓扑、文件/文件夹与网页保存为可复用的「工作空间」，随时一键切换整套桌面工作环境。

[![Release](https://img.shields.io/github/v/release/fanfanhappy/workscene?label=Release&color=0071e3)](https://github.com/fanfanhappy/workscene/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011-0078D6)]()
[![Stack](https://img.shields.io/badge/Stack-Tauri%202%20%C2%B7%20React%2019%20%C2%B7%20Rust-111827)]()
[![Privacy](https://img.shields.io/badge/Privacy-100%25%20Local%20Offline-22c55e)]()

</div>

---

## 核心特性

- **现场捕获与 L2 上下文识别**
  毫秒级枚举可见、最大化与最小化窗口，过滤虚拟桌面/系统幽灵窗；自动识别已打开的 Windows 文件资源管理器目录及 VS Code / Cursor / Windsurf / Trae / Antigravity 项目路径，支持全局捕获忽略名单。
- **一键精准恢复与 0px 贴边分屏**
  按优先级与启动延迟自动补启缺失应用（支持多开参数注入），结合 `DWMWA_EXTENDED_FRAME_BOUNDS` 隐形边框补偿实现像素级贴边；可选「专注模式」收起无关窗口，或一键「收工」优雅关闭本空间窗口。
- **可视化画布与多屏幕拓扑方案**
  内置可拖拽缩放的工作区画布（支持左右平分、左大右小、三列均分、四宫格、居中叠放及 `Ctrl+Z/Y` 撤销重做）；同一工作空间可为单屏、双屏、扩展屏分别保存独立布局，恢复时自动匹配当前屏幕组合。
- **资源联动与自动化钩子**
  支持关联常用文件、文件夹、批量网址及 Chrome / Edge 书签夹（纯本地只读，不读密码/Cookie）；可选配置「恢复前 / 收工后」本机命令钩子，衔接本地开发服务或环境切换。
- **历史快照与可诊断恢复报告**
  每次恢复前自动生成保护快照，支持随时撤销、历史版本差异对比、另存为独立工作空间以及单条失败项精准重试。
- **高频快捷入口**
  提供浅色 / 深色 / 跟随系统主题、`Ctrl+K` 全局命令面板、系统托盘快捷菜单、零轮询全局热键（`Ctrl+Alt+W` 唤起主窗口、`Ctrl+Alt+1~9` 直达工作空间）与全量 JSON 备份迁移。

---

## 下载与安装

前往 [GitHub Releases](https://github.com/fanfanhappy/workscene/releases/latest) 下载最新版本（当前正式版 **v1.1.3**）：

| 版本形态 | 文件名 | 说明 |
|---|---|---|
| **标准安装版** | `WorkScene_1.1.3_x64-setup.exe` | NSIS 当前用户安装，无需管理员权限，数据存 `%LOCALAPPDATA%\WorkScene` |
| **绿色便携版** | `WorkScene-1.1.3-portable.zip` | 解压后双击 `start.cmd` 运行，数据自动存同目录 `data\`，拷走即迁移 |

> **SmartScreen 提示**：当前开源版本尚未进行商业代码签名。首次运行如遇 Windows SmartScreen 拦截，点击「更多信息 → 仍要运行」即可。

---

## 隐私与能力边界

- **100% 本地离线**：所有配置、数据库（SQLite WAL）与快照文件均保存在本机，无云同步、无遥测、不截屏、不读取浏览器密码或 Cookie。仅在用户主动点击「设置 → 关于 → 检查新版本」时访问 GitHub Releases API。
- **Windows 系统边界**：
  - 受 Windows UIPI 权限隔离限制，普通权限下无法移动以管理员身份运行的窗口；
  - 暂不支持 UWP 打包应用（`ApplicationFrameWindow`）的窗口恢复。

---

## 本地开发与构建

### 环境要求

- Windows 10 / 11（x64）
- Node.js 20+、`pnpm` 9+
- Rust 1.77+（MSVC 工具链）与 WebView2 Runtime

### 常用命令

```powershell
# 安装前端依赖
pnpm install

# 启动开发环境（Vite + Tauri）
pnpm tauri dev

# 运行 Rust 单元测试与前端类型检查
cd src-tauri; cargo test --lib; cd ..
pnpm build

# 构建 Release 可执行文件与 NSIS 安装包
pnpm tauri build
```

### 技术栈

- **前端**：React 19 · TypeScript · Vite 6 · Tailwind CSS 4 · TanStack Query · Zustand · Lucide Icons
- **后端**：Tauri 2 · Rust · `windows-sys` (Win32 / DWM / Per-Monitor V2 DPI) · SQLite (`sqlx`)

---

## 发布记录

完整版本更新日志与技术说明请参阅 [docs/RELEASE.md](docs/RELEASE.md) 或 [GitHub Releases](https://github.com/fanfanhappy/workscene/releases)。
