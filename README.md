# WorkScene

Windows 桌面工作现场管理器：保存整个工作现场（应用、窗口布局、显示器配置），需要时一键恢复。

依据文档：

- PRD / 技术架构 `WorkScene_Product_Requirements_and_Technical_Architecture_v1.0.docx`
- 前端 UI/UX 规范 `WorkScene_Frontend_UI_UX_Design_Guidelines_v1.0.docx`
- 评审：`docs/reviews/`（v1.0 评审、UI/UX 评审、补充意见）
- 决策记录：`docs/adr/`（文件存储、匹配权重）
- 恢复契约：`docs/RESTORE_CONTRACT.md`

## 技术栈

Tauri 2 · React 19 + TypeScript · Vite 6 · Tailwind CSS 4（设计令牌）· TanStack Query · Rust（windows-sys / Win32）· 快照为 JSON（ADR-0001 说明为何 M1 不建库）

## 进度：M1 完成（含最大化状态）

| PRD Day | 内容 | 状态 |
|---|---|---|
| 1 | 工程初始化、React ↔ Rust IPC | 完成 |
| 2 | 窗口枚举（HWND/PID/标题/类名/矩形）+ 系统窗口过滤 | 完成 |
| 3 | 窗口状态与显示器（分辨率、工作区、DPI、主屏） | 完成 |
| 4 | 窗口恢复（位置 + 最大化）+ 偏差校验 | 完成，偏差 0px |
| 5 | 应用启动（按快照实例数补齐） | 完成（无启动参数） |
| 6 | 捕获 → 保存 → 破坏现场 → 恢复闭环 | 完成，2 窗口含 1 个最大化 |
| 7 | 双屏 / 跨 DPI / 部分失败报告 | 部分（本机单屏） |

### 按评审已修的问题

| 评审项 | 处理 |
|---|---|
| 补充意见 **D1**：`GetWindowPlacement` 覆盖 `showCmd`，最大化永不生效且报成功 | 已修：先读后写；新增 `IsZoomed` 校验，最大化失败判 `failed`；`--m1test` 覆盖最大化 |
| **D2**：最小化被静默还原 | 已修：`stateRestored` 字段 + 明确文案，不再谎报完全成功 |
| **D3**：候选不消耗，同一窗口被移动两次 | 已修：任务内 `used` 集合 + 按 exe 补齐实例数 |
| **D4**：只比对位置不比尺寸 | 已修：四边 + 尺寸，阈值 `max(8, 2×scale)` |
| **D5**：报告不落盘 | 已修：`runs/<runId>.json` + `run_list` |
| **A3**：非原子写、损坏静默消失 | 已修：`tmp → bak → rename` + `scene_health` 诊断上报 |
| **A8**：恢复不可撤销 | 已修：恢复前自动保护快照 + 报告页「撤销这次恢复」 |
| R2 DPI | PMv2 已启用；最小化取 `rcNormalPosition`；`SetThreadDpiAwarenessContext` 未做（见下） |
| R5 导入即执行 | 尚无导入功能，但 `launch_app` 无二次确认 —— 未处理 |
| R6 指纹/自动快照、R7 单实例、R8 成功率基线 | 未做 |

### 实测证据（本机，2026-11-21）

```
$ cargo run -- --m1test
[1/6] 模拟窗口：A HWND … 普通，B HWND … 最大化=true
[2/6] 已捕获并保存快照：2 个窗口（含 1 个最大化），1 台显示器
[3/6] 已破坏现场：A 移到 (900, 480)，B 取消最大化=true
[5/6] 校验：A 目标 (240, 160) 实际 (240, 160, 900, 620)；B 最大化=true
[6/6] M1 结果：PASS（成功 2 / 失败 0 / 跳过 0）
```

- 捕获：428 个顶层窗口 → 保留 9 个真实工作窗口，排除 419 个（不可见/工具窗口/系统 Shell/UWP）
- 单元测试：14 passed（比例坐标、负坐标副屏、边界收敛、最小尺寸、歧义跳过、类名不符拒绝、路径穿越、DPI 阈值）
- 前端：`tsc --noEmit` + `vite build` 通过；桌面端真实启动并截图（`docs/screenshot-overview.png`）

## 运行

```bash
pnpm install
pnpm tauri dev                       # 开发

# 后端自检（不启动 UI）
cargo run --manifest-path src-tauri/Cargo.toml -- --selftest      # 打印真实捕获 JSON
cargo run --manifest-path src-tauri/Cargo.toml -- --m1test        # 捕获→破坏→恢复→校验（含最大化）
cargo run --manifest-path src-tauri/Cargo.toml -- --capture "名称" # 无头捕获并保存快照
cargo test --manifest-path src-tauri/Cargo.toml

pnpm tauri build                     # 打包 MSI / NSIS
```

## 数据

```
%LOCALAPPDATA%\WorkScene\
├── snapshots\scene-<id>.json    # 不可变快照（原子写，保留 .bak）
├── runs\run-<id>.json           # 恢复报告
└── webview2\                    # WebView2 用户数据
```

- `WORKSCENE_DATA_DIR` 可覆盖数据根目录（便携模式 / 测试）
- 快照只含窗口元数据：标题、进程路径、类名、矩形、显示器、DPI
- 不读取文件内容、密码、Cookie、键盘输入，不截图，不上传

## 目录

```
src/                    前端（components / pages / services / types / styles）
src-tauri/src/
├── win/                window · process · display · dpi · filter · mock
├── capture/            捕获装配
├── restore/            layout（纯函数）· matcher（评分）· 编排
├── storage/            快照与报告落盘（原子写、id 白名单）
└── commands/           Tauri Command 边界
```

`win/mock.rs` 是受控的模拟 Win32 窗口，供 `--m1test` 使用：不依赖 notepad（Win11 上已是打包应用，按 v1 范围被过滤），也不依赖外部进程。

## 已知边界（v0.1，不要当成已支持）

- **不支持 UWP / 打包应用**（`ApplicationFrameWindow` 显式排除；Win11 的记事本、设置、计算器都是这一类）
- 不支持虚拟桌面归属恢复：窗口恢复到当前虚拟桌面
- 应用启动不支持自定义参数
- **未在真实双屏 / 跨 DPI 环境验证**（本机单屏 3840×2160 @150%）
- 未调用 `SetThreadDpiAwarenessContext`：目标进程 DPI 感知模式与 WorkScene 不一致时可能错位
- 快照明文保存窗口标题与完整路径，**没有加密**；导入/导出未实现（实现时必须按评审 R5 补信任模型）
- 无单实例互斥；同一时刻发起两次恢复会互相干扰
- 无自动快照、无指纹去重、无成功率基线
- 最小化窗口恢复为普通窗口（有 `stateRestored=false` 与文案说明，不算谎报）

## 下一步（M2）

SQLite + SQLx 最小表集（按评审修正约束与索引）、工作空间与应用配置 CRUD、应用启动参数与 `reuse_policy`、`error_code` 契约落地（见 `docs/RESTORE_CONTRACT.md` 第 3 节）、单实例与恢复互斥、10 应用 × 3 场景成功率基线。
