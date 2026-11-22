# WorkScene v1.1.1 正式版发布说明

- 日期：2026-11-22
- 形态：Windows 10/11 桌面应用（提供 NSIS 单文件安装包 `WorkScene_1.1.1_x64-setup.exe` 与免安装便携版 `WorkScene-1.1.1-portable.zip`）
- 数据：100% 本地离线（`%LOCALAPPDATA%\WorkScene` 或便携目录 `data\`），无遥测、无云同步、不截屏、不读取浏览器密码/Cookie

## v1.1.0 重磅升级

- **真正的 L2 上下文自动捕获**：扫描桌面时自动识别已打开的 Windows 文件资源管理器文件夹路径，以及 VS Code / Cursor / Windsurf / Trae / Antigravity 当前打开的项目目录，勾选即可一键保存为工作空间资源。
- **像素级 0px 贴边分屏**：引入 `DWMWA_EXTENDED_FRAME_BOUNDS` 隐形阴影边框自动补偿，消除 Win10/11 左右分屏与贴边留缝问题。
- **多屏幕拓扑布局方案（Display Profiles）**：同一工作空间可为「单屏 / 双屏 / 扩展屏」分别保存独立窗口排布，恢复时按当前显示器组合自动匹配。
- **生命周期钩子（进阶自动化）**：支持为工作空间可选配置「一键恢复前 / 一键收工后」本地命令（默认关闭），轻松联动本地开发服务或环境切换。
- **零轮询热键线程 & 后台状态反馈**：全局快捷键线程升级为阻塞式 `GetMessageW` + `PostThreadMessageW`（0 轮询 CPU 唤醒，< 1ms 响应）；托盘恢复异常或繁忙时实时通知前端并更新托盘提示。
- **画布与交互升级**：`MonitorCanvas` 缩略图直接展示软件名与全屏/最小化标识；编辑工作空间信息弹窗直接支持绑定 `Ctrl+Alt+1~9` 快捷槽；资源列表支持内联编辑与删除确认；「设置 → 关于」新增手动检查 GitHub 新版本。

## 交付内容

### 核心能力

| 能力 | 说明 |
|---|---|
| 现场捕获 | 毫秒级扫描可见顶层窗口（含正常/最大化/最小化窗口与 Chrome/Edge 等浏览器），真实提取 `.exe` 图标，支持全局忽略名单 |
| 工作空间 | 应用清单 + 窗口比例布局 + 文件/文件夹/网址资源；支持收藏置顶、色标分组、复制、导入预检/导出 JSON、批量粘贴网址与 Chrome/Edge 书签夹一键导入 |
| 布局画布 | 拖拽调位置/尺寸、跨屏换算、5 种一键分屏排版模板（左右平分/左大右小/三列均分/四宫格/居中叠放）、`Ctrl+Z/Y` 撤销重做、`Ctrl+S` 保存与跨页未保存保护 |
| 恢复中心 | 智能一键恢复、自动补齐未运行程序（支持优先启动与延迟等待）、专注模式（自动最小化非本空间窗口）、一键收工（优雅 `WM_CLOSE`）、取消恢复、仅重试未完成项、恢复前自动保护快照与一键撤销 |
| 历史与快照 | 手动/自动/保护快照时间线、可视化布局预览、差异对比、恢复记录逐步诊断、异常退出恢复提示 |
| 快捷与常驻 | Apple macOS 浅色/深色/跟随系统主题（含原生标题栏同步）、首次安装欢迎动画与 4 步上手指引、`Ctrl+K` 命令面板、托盘一键恢复、持久化 `Ctrl+Alt+1~9` 工作空间快捷槽 |
| L2 适配 | 内置 VS Code/Cursor/Antigravity/Windsurf/Trae/JetBrains、Windows Terminal(`-d`)、Chrome/Edge/Firefox/Brave/Arc/Vivaldi、Explorer 适配与全局忽略名单 |

### 工程

- Rust 后端：`win/`（窗口、进程、显示器、DPI、过滤、模拟窗口、单实例、托盘、热键、开机启动）、`capture/`、`restore/`（layout / matcher / 编排）、`data/`（SQLite 迁移、工作空间、快照、报告、设置、会话）、`storage/`、`commands/`
- 前端：React 19 + TS（strict）+ Tailwind 4 设计令牌 + TanStack Query；4 个页面 + 设置页 + 历史页
- 数据：SQLite（WAL，外键开启）存关系数据；快照 payload 为不可变 JSON 文件（见 ADR-0001 / 0003）
- 迁移：`src-tauri/migrations/0001_init.sql`

## 验证结果（2026-11-21，本机）

```
cargo test                46 passed
--m1test                  PASS（普通+最大化窗口，0px）
--m2test                  PASS（建空间 → 恢复 → 报告入库 → 级联删除）
--m5test                  PASS（指纹去重 / 节流 / 保留 / 差异）
--m9test                  PASS（4 个资源，1 个不可达被正确标出）
--baseline 3              100%（模拟窗口，门槛 95%）
--baseline-live 3         3/3，0px（真实窗口，报告与实测一致）
```

- 真实窗口样本：Chrome / Antigravity / DeepSeek Harness / ChatGPT（Chromium 系）
- 前端：`tsc --noEmit` + `vite build` 通过；桌面端真实启动截图见 `docs/screenshot-*.png`

## 已知限制（发布前必读）

1. **不支持 UWP / 打包应用**（Win11 记事本、设置、计算器都属于这一类），捕获时显式排除并给出原因。
2. **未在真实双屏 / 混合 DPI 环境验证**：本机只有单屏 3840×2160 @150%。代码路径已实现（显示器匹配、比例换算、线程级 DPI 上下文），但没有实机数据。
3. **开机启动未端到端验证**：本沙箱进程写 `HKCU\…\Run` 被拒（Win32 5），PowerShell 与 reg.exe 可写，判断为执行环境令牌限制。UI 会如实报错。
4. **安装包未签名**：SmartScreen 会拦截一次；签名步骤见 `docs/PACKAGING.md`。
5. **没有自动更新**。
6. 恢复记录最多显示最近 10 次；快照保留按工作空间计。
7. 最小化窗口恢复为普通窗口（报告里以 `stateRestored=false` + 文案说明，不算谎报）。
8. 全局快捷键只支持 4 个预设，不支持任意组合录入。

## 与文档的对应关系

- PRD：M1–M6 全部落地（M6 的多显示器适配为"已实现未实机验证"）
- UI/UX 规范：设计令牌、三态恢复中心、能力分级、历史时间线、布局编辑器、Command Palette（Ctrl K）、设置分段页均已实现；未做的是页面级细节打磨
- 评审意见：D1–D5、A3、A8、R2、R5（部分）、R6、R7、R8 已处理，详见 README「按评审已修的问题」

## 升级与数据兼容

- 快照 JSON 带 `schemaVersion`，`load_scene` 拒绝来自更高版本的快照并提示升级
- 数据库迁移只追加，不改历史迁移
- 卸载不删除 `%LOCALAPPDATA%\WorkScene`（用户数据保留）
