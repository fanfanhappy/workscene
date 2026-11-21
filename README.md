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

## 进度：M2（工作空间 + SQLite），M1 闭环已验证

### M1 系统能力验证

| PRD Day | 内容 | 状态 |
|---|---|---|
| 1 | 工程初始化、React ↔ Rust IPC | 完成 |
| 2 | 窗口枚举（HWND/PID/标题/类名/矩形）+ 系统窗口过滤 | 完成 |
| 3 | 窗口状态与显示器（分辨率、工作区、DPI、主屏） | 完成 |
| 4 | 窗口恢复（位置 + 最大化）+ 偏差校验 | 完成，偏差 0px |
| 5 | 应用启动（按快照实例数补齐） | 完成（无启动参数） |
| 6 | 捕获 → 保存 → 破坏现场 → 恢复闭环 | 完成，2 窗口含 1 个最大化 |
| 7 | 双屏 / 跨 DPI / 部分失败报告 | 部分（本机单屏） |

### M2 工作空间模型

| 内容 | 状态 |
|---|---|
| SQLite（`workscene.db`）+ sqlx 迁移，WAL + 外键开启 | 完成 |
| 迁移 `0001_init.sql`：workspaces / workspace_items / app_configs / window_layouts / snapshots / restore_runs / restore_steps / app_settings | 完成 |
| 从一次捕获创建工作空间：应用按可执行路径去重，窗口逐条存比例坐标 | 完成 |
| 工作空间列表 / 详情 / 重命名 / 删除（级联 + 清理 payload 文件） | 完成 |
| 从工作空间恢复（DB 布局 → 恢复计划，恢复前自动保护快照） | 完成 |
| 恢复报告入库（run + 逐步 step，含 `seq` 与 `error_code`），按错误码聚合 | 完成 |
| 按快照恢复（撤销这次恢复 / 历史回退） | 完成 |
| UI：工作空间页（列表 + 详情 + 应用清单）、捕获改为"保存为工作空间" | 完成 |
| 工作空间编辑（增删应用、拖拽布局）、`reuse_policy` 配置项 | 未做（M3） |

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

$ cargo run -- --m2test
[1/7] 数据库已初始化并跑完迁移
[2/7] 已建工作空间：1 个应用项 / 2 个窗口（ws-…）
[3/7] 列表 1 个；详情：1 个应用项、2 个窗口、首个显示器 \\.\DISPLAY5
[4/7] 已由 DB 布局构造恢复计划：2 个窗口
[5/7] 恢复结果：成功 2 / 失败 0 / 跳过 0
[6/7] 校验：A 位置 通过（目标 260,180），B 最大化=true
[7/7] 报告入库=true，runs=1，首条 steps=2，错误码分布=[("ok", 2)]
     删除=true，残留 items=0，快照文件 3 → 2
M2 结果：PASS（工作空间闭环 + 报告入库 + 级联清理）
```

- 捕获：428 个顶层窗口 → 保留 9 个真实工作窗口，排除 419 个（不可见/工具窗口/系统 Shell/UWP）
- 单元测试：19 passed（坐标比例、负坐标副屏、边界收敛、最小尺寸、歧义跳过、类名不符拒绝、路径穿越、DPI 阈值、应用去重、级联删除、唯一约束、scenario 往返）
- 前端：`tsc --noEmit` + `vite build` 通过；桌面端真实启动并截图（`docs/screenshot-workspaces.png`）

## 运行

```bash
pnpm install
pnpm tauri dev                       # 开发

# 后端自检（不启动 UI）
cargo run --manifest-path src-tauri/Cargo.toml -- --selftest      # 打印真实捕获 JSON
cargo run --manifest-path src-tauri/Cargo.toml -- --m1test        # 捕获→破坏→恢复→校验（含最大化）
cargo run --manifest-path src-tauri/Cargo.toml -- --m2test        # 工作空间闭环：建库→建空间→恢复→报告入库→删除
cargo run --manifest-path src-tauri/Cargo.toml -- --capture "名称" # 无头捕获并保存为工作空间
cargo test --manifest-path src-tauri/Cargo.toml

pnpm tauri build                     # 打包 MSI / NSIS
```

## 数据

```
%LOCALAPPDATA%\WorkScene\
├── workscene.db                 # SQLite：工作空间 / 布局 / 快照索引 / 恢复报告（WAL）
├── snapshots\scene-<id>.json    # 快照 payload（不可变，原子写，保留 .bak）
└── webview2\                    # WebView2 用户数据
```

- `WORKSCENE_DATA_DIR` 可覆盖数据根目录（便携模式 / 测试）
- **混合存储**：关系数据在 SQLite，快照 payload 是独立 JSON 文件（见 `docs/adr/0001`、`docs/adr/0003`）
- 快照只含窗口元数据：标题、进程路径、类名、矩形、显示器、DPI
- 不读取文件内容、密码、Cookie、键盘输入，不截图，不上传

## 目录

```
src/                    前端
├── components/         AppShell · MonitorCanvas · UI 基础组件
├── pages/              Overview · Workspaces · Capture · Restore
├── services/api.ts     唯一 invoke 出口
├── types/contract.ts   IPC 契约（与 Rust 模型一一对应）
└── styles/index.css    设计令牌（含浅色主题、对比度修正）
src-tauri/
├── migrations/         0001_init.sql（M2 最小表集）
└── src/
    ├── win/            window · process · display · dpi · filter · mock
    ├── capture/        捕获装配
    ├── restore/        layout（纯函数）· matcher（评分）· 编排
    ├── data/           SQLite 连接与迁移 · workspaces 仓储 · runs 归档
    ├── storage/        快照 payload 落盘（原子写、id 白名单）
    └── commands/       Tauri Command 边界
```

`win/mock.rs` 是受控的模拟 Win32 窗口，供 `--m1test` / `--m2test` 使用：不依赖 notepad（Win11 上已是打包应用，按 v1 范围被过滤），也不依赖外部进程。

## 已知边界（v0.2，不要当成已支持）

- **不支持 UWP / 打包应用**（`ApplicationFrameWindow` 显式排除；Win11 的记事本、设置、计算器都是这一类）
- 不支持虚拟桌面归属恢复：窗口恢复到当前虚拟桌面
- 应用启动不支持自定义参数；`reuse_policy` 只有默认值，UI 还不能改
- 工作空间**不能编辑**：只能整体重建（重新捕获）或删除
- **未在真实双屏 / 跨 DPI 环境验证**（本机单屏 3840×2160 @150%）
- 未调用 `SetThreadDpiAwarenessContext`：目标进程 DPI 感知模式与 WorkScene 不一致时可能错位
- 快照 JSON 明文保存窗口标题与完整路径，**没有加密**；导入/导出未实现（实现时必须按评审 R5 补信任模型）
- 无单实例互斥；同一时刻发起两次恢复会互相干扰
- 无自动快照、无指纹去重（`content_fingerprint` 已入库，但目前只记录不参与去重）
- 最小化窗口恢复为普通窗口（有 `stateRestored=false` 与文案说明，不算谎报）

## 下一步（M3）

工作空间编辑（增删应用、可视化拖拽布局 UI-06b）、`reuse_policy` 三值与启动参数、`SetThreadDpiAwarenessContext`、单实例与恢复互斥、10 应用 × 3 场景成功率基线（把 M1 的 go/no-go 门补上）。
