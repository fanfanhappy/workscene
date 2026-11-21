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

## 进度：M13（快捷键可配置）；M1–M12 已完成

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
| 工作空间编辑（增删应用、拖拽布局） | **已完成（M4，见下）** |

### M3 安全与可测量性

| 内容 | 状态 |
|---|---|
| 单实例：第二个实例把已有窗口拉到前台后退出（`CreateMutexW`） | 完成，实测双开只剩 1 个进程 |
| 全局恢复互斥：同时发起两次恢复会被拒绝，UI 轮询 `restore_is_busy` 禁用按钮 | 完成 |
| `reuse_policy` 三值语义：`reuse_if_matched` / `always_launch` / `reuse_or_launch` | 完成，含 3 条单测 |
| 应用启动参数（类 shell 切分、按数组传给 CreateProcess，绝不拼 cmd） | 完成，含切分单测 |
| 应用项配置 UI：复用策略下拉 + 启动参数输入（失焦保存） | 完成 |
| 快照比例坐标归一化（最大化窗口实测 -0.3%/100.6% 越界） | 完成，含 2 条单测 |
| 成功率基线协议 `--baseline N`（JSONL 明细 + 错误码分布 + 95% 门槛） | 完成，5 轮 15 项 100% |
| 10 应用 × 3 场景真实矩阵 | 未做（需真实应用与双屏） |

### M4 可编辑布局与跨 DPI

| 内容 | 状态 |
|---|---|
| 可视化布局编辑：拖动窗口改位置、拖右下角改尺寸，跨屏拖动按各自工作区换算 | 完成 |
| 批量保存：一次事务提交多个窗口比例坐标；被移除的窗口单独删行 | 完成 |
| 边界约束：`sanitize_layout` 保证 [0,1]、最小 8%、非有限值不落库 | 完成，含单测 |
| 删除应用项（级联删其窗口）与删除单个窗口 | 完成，含级联单测 |
| `SetThreadDpiAwarenessContext`：每次 SetWindowPos/SetWindowPlacement 前把线程切到 PMv2 | 完成 |
| 布局保存后行更新与「放弃改动」回滚 | 完成 |

### M5 Hybrid 快照

| 内容 | 状态 |
|---|---|
| 手动快照（每次都写）与自动快照（指纹去重 + 180s 节流） | 完成，见 ADR-0004 |
| 内容指纹：路径/类名/显示器/位置桶(40px)/尺寸/状态，标题不入指纹 | 完成 |
| 保留策略：只淘汰最旧的 auto，手动/初始/保护快照永不删除，同时删 payload 文件 | 完成 |
| 后台自动快照任务：仅在存在激活工作空间且开关打开时工作，60s 巡检 | 完成 |
| 恢复成功即记录激活工作空间（自动快照的归属） | 完成 |
| 快照差异：按「进程+类名+归一化标题」对齐，输出新增/移除/移动/状态变化 | 完成，含 2 条单测 |
| 历史页：时间线、类型标签、保护标记、比较、恢复、删除、清理自动快照 | 完成 |
| 崩溃后恢复提示（异常退出检测） | **已完成（M6）** |

### M6 异常退出恢复与 L2 适配

| 内容 | 状态 |
|---|---|
| 会话标记：启动写 `session.json(clean=false)`，正常退出改 clean | 完成 |
| 异常退出检测：启动读到未清理标记 + 激活工作空间 + 最近快照，生成 `recovery-hint.json` | 完成，实测强杀后重启命中 |
| 总览页恢复横幅：显示崩溃时间与可恢复快照，「恢复上次现场」/「忽略」 | 完成 |
| L2 适配建议：按可执行文件名给 VS Code / Terminal / 浏览器 / Explorer 的建议参数 | 完成，含 4 条单测 |
| 适配建议只做提示不自动填（避免猜错参数把恢复变成乱开窗口），用户点「建议参数」才写入 | 完成 |
| 系统托盘 / 全局快捷键 | **已完成（M7）** |
| 安装包与签名配置 | **NSIS 打包完成，签名未做**（见 `docs/PACKAGING.md`） |

### M7 常驻形态与分发

| 内容 | 状态 |
|---|---|
| 系统托盘：左键唤起、右键菜单（显示主窗口 / 立即快照 / 退出） | 完成 |
| 关闭主窗口 = 隐藏到托盘，退出只能走托盘菜单（避免找不到退出入口） | 完成，实测 WM_CLOSE 后进程存活 |
| 全局快捷键 Ctrl+Alt+W 唤起窗口（Win32 RegisterHotKey + 独立消息循环，无第三方插件） | 完成 |
| 打包配置：NSIS 当前用户安装、中英双语、WebView2 downloadBootstrapper、publisher/copyright/描述 | 完成 |
| 签名流程文档（signtool + 时间戳 + 发布检查清单） | 完成，见 `docs/PACKAGING.md` |
| 自动更新、崩溃转储 | 未做 |

### M8 设置页与可配置策略

| 内容 | 状态 |
|---|---|
| 设置页：自动快照开关 / 保留份数 / 最小间隔、恢复默认行为、存储统计、快捷键、关于 | 完成 |
| 设置存 `app_settings`，读取带默认值并做区间约束（保留 1–500、间隔 10–86400s） | 完成，含 2 条单测 |
| 后台自动快照改为读取设置（开关、间隔、保留）而非写死常量 | 完成 |
| 存储统计：数据库含 WAL 体积、快照数量与占用、恢复报告条数 | 完成 |
| 一键按当前保留策略清理全部旧自动快照 | 完成 |
| 设置入口放在侧栏底部（与 UI 规范 §3.2 一致） | 完成 |
| 快捷键自定义、开机启动开关 | 未做（快捷键写死 Ctrl+Alt+W） |

### M9 资源恢复（PRD FR-008 / FR-009）

| 内容 | 状态 |
|---|---|
| 工作空间可加文件 / 文件夹 / 网址（地址栏输入，不依赖文件选择器插件） | 完成 |
| 入库校验：类型白名单、网址必须 http(s)、同空间去重、空值拒绝 | 完成，含单测 |
| 恢复流程：窗口摆好后再打开资源（避免抢焦点），ShellExecuteW 走系统默认关联 | 完成 |
| 报告区分 `window` / `resource` 两类步骤，失败给 `resource_open_failed` | 完成 |
| 预览判定：文件/文件夹看存在性，网址看协议；不可达直接标红 | 完成 |
| 列表与详情区分「应用 / 资源 / 窗口」三个计数（之前资源被算进应用） | 完成 |
| 抽样自检 `--m9test`：4 个资源（1 个故意不存在）→ 预览判定 1 个不可达 | 完成 |

### M10 开机启动与资源打开方式

| 内容 | 状态 |
|---|---|
| 开机启动：写 `HKCU\\…\\CurrentVersion\\Run`，值带引号，关闭时精确删除 | 完成，**本环境无法端到端验证**（见下） |
| 设置页「启动」分组开关，写入失败会把 Win32 错误显示出来 | 完成 |
| 诊断入口 `--autostart status\\|on\\|off`，失败时打印 Win32 错误码 | 完成 |
| 资源打开方式：工作空间里带 VS Code/Cursor 时，文件夹交给 IDE 打开（`code <dir>`），文件与网址仍走默认关联 | 完成，含 2 条单测 |

### M11 恢复记录回看

| 内容 | 状态 |
|---|---|
| 历史页新增「恢复记录」：最近 10 次 run，状态（全部成功/部分成功/失败）+ 计数 + 触发方式 | 完成 |
| 展开看逐步结果：序号、窗口/资源类型、目标、消息、`error_code`、耗时 | 完成 |
| `run_steps(run_id)` 命令 + `data::runs::steps`，关窗重启后仍可回查 | 完成，含 2 条单测 |
| 顺带修掉：`restore_steps.step_type` 写死为 window，资源步骤在记录里被标错 | 已修（测试先抓到） |
| 顺带修掉：同一毫秒连续添加资源时 `workspace_items.id` 冲突 | 已修（测试先抓到） |

### M12 真实窗口成功率基线

| 内容 | 状态 |
|---|---|
| `--baseline-live N`：用当前真实窗口建空间 → 扰动 → 恢复 → **独立复核**偏差 | 完成 |
| 实测：4 个真实窗口（Chrome×2 / Antigravity / DeepSeek Harness）全部 0px 复原，成功率 100% | 完成 |
| 顺带修掉：Chromium 系窗口 `SetWindowPlacement` 返回 FALSE 但实际已移动，旧代码据此报失败 | 已修（实测先抓到） |
| 多显示器 / 混合 DPI 场景 | 未做（本机单屏） |

### M13 快捷键可配置

| 内容 | 状态 |
|---|---|
| 快捷键预设：Ctrl+Alt+W / Ctrl+Shift+W / Ctrl+Alt+Space / 关闭 | 完成，含 3 条单测 |
| 运行时切换：RegisterHotKey 是线程绑定的，热键线程持有通道，设置页只发"换成哪个" | 完成 |
| 白名单校验：未知组合不会写进设置，回落到默认 | 完成 |
| 注册失败（被占用）时如实报错，不假装已生效 | 完成 |
| 设置页顺序调整为 自动快照 → 启动 → 快捷键 → 恢复 → 存储 → 关于 | 完成 |

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
| R7 单实例 / 并发恢复 | 已修：进程级互斥 + 恢复互斥锁 |
| R8 成功率基线 | 已实现协议 `--baseline`（模拟窗口样本，见 `docs/BASELINE.md`） |
| R6 指纹/自动快照 | 未做（`content_fingerprint` 已入库，未参与去重） |

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
cargo run --manifest-path src-tauri/Cargo.toml -- --m5test       # 快照去重/节流/保留/差异
cargo run --manifest-path src-tauri/Cargo.toml -- --m9test       # 资源（文件/文件夹/网址）入库与预览判定
cargo run --manifest-path src-tauri/Cargo.toml -- --baseline 5    # 模拟窗口基线（JSONL + 汇总，95% 门槛）
cargo run --manifest-path src-tauri/Cargo.toml -- --baseline-live 6 # 真实窗口基线（会临时移动已开窗口，恢复后复原）
cargo run --manifest-path src-tauri/Cargo.toml -- --snap "名称"    # 给同名工作空间追加一份手动快照（并设为激活空间）
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
- 应用启动参数由用户填写原始字符串，只支持引号分组，不支持转义与变量展开
- 工作空间**不能编辑**：只能整体重建（重新捕获）或删除
- **未在真实双屏 / 跨 DPI 环境验证**（本机单屏 3840×2160 @150%）
- 跨 DPI 已加线程级 context 切换，但**真实双屏 + 混合 DPI 仍未实机验证**
- 快照 JSON 明文保存窗口标题与完整路径，**没有加密**；导入/导出未实现（实现时必须按评审 R5 补信任模型）
- 单实例与恢复互斥已实现，但窗口标题写死为 "WorkScene"（多语言改名后需同步）
- 关闭窗口会隐藏到托盘；退出必须用托盘菜单（或任务管理器）\n- 安装包未签名，分发时会被 SmartScreen 提示\n- 快捷键有 4 个预设（含关闭），暂不支持任意组合录入\n- 开机启动在**本沙箱环境写注册表被拒（Win32 错误码 5）**：PowerShell 与 reg.exe 能写，本程序进程不能，判断是执行环境令牌限制，未能在真机验证端到端；UI 会如实报错\n- 自动快照的开关/间隔/保留已可配置，但全局开关与每工作空间开关是两层（都需打开）\n- 自动快照只在"有激活工作空间"时工作；应用没运行的应用不会被自动记录\n- 指纹用 DefaultHasher，跨版本不稳定，仅用于同一次运行内去重
- 最小化窗口恢复为普通窗口（有 `stateRestored=false` 与文案说明，不算谎报）

## 下一步（M14）

终验收：全量自检 + 安装包重建 + 发布说明；多显示器与 Office/IDE 真实样本需要外部环境。
