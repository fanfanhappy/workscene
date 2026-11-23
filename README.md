<div align="center">

# WorkScene

**Windows 10 / 11 本地优先桌面工作现场管理器**

将常用应用、窗口布局、多显示器拓扑、项目目录与网页保存为可复用的「工作空间」，随时一键切换整套桌面工作环境。

[![Release](https://img.shields.io/github/v/release/fanfanhappy/workscene?label=Release&color=0071e3)](https://github.com/fanfanhappy/workscene/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011%20(x64)-0078D6)]()
[![Stack](https://img.shields.io/badge/Stack-Tauri%202%20%C2%B7%20React%2019%20%C2%B7%20Rust-111827)]()
[![Privacy](https://img.shields.io/badge/Privacy-100%25%20Local%20Offline-22c55e)]()

[下载安装包](https://github.com/fanfanhappy/workscene/releases/latest) · [发布说明](docs/RELEASE.md) · [快速上手](#快速上手) · [快捷键速查](#快捷键速查) · [架构与开发](#架构与本地开发)

</div>

---

## 为什么需要 WorkScene？

在日常开发、设计、写作与多任务办公中，每次切换任务或重启电脑后，往往需要重复经历：

1. 逐个寻找并打开 5~10 个软件、IDE 项目、终端与参考文档；
2. 在多个显示器之间手动拖拽、分屏、对齐每一个窗口；
3. 切换另一项工作时，又要把当前满屏窗口逐个收起或关闭。

**WorkScene** 将整套桌面现场抽象为可持久化的 **工作空间（Workspace）**——毫秒级捕获当前桌面，一键按优先级拉起缺失程序、恢复像素级贴边布局、打开关联的项目目录与网页；并在每次恢复前自动生成保护快照，确保任何操作都**可预测、可诊断、随时可撤销**。

---

## 核心特性

### 1. 现场捕获与 L2 上下文自动识别
- **毫秒级窗口扫描**：基于 Win32 原生 API 快速枚举可见、最大化及最小化窗口，自动通过 `DWMWA_CLOAKED` 过滤其他虚拟桌面与系统后台幽灵窗口。
- **L2 上下文自动提取**：扫描桌面时自动识别当前已打开的 **Windows 文件资源管理器目录**，以及 **VS Code / Cursor / Windsurf / Trae / Antigravity** 正在编辑的本地项目路径，保存时自动关联为启动参数或目录资源。
- **全局忽略与降噪**：内置系统噪声窗口过滤规则，支持在设置中将聊天工具、音乐播放器等常驻软件加入全局捕获忽略名单。

### 2. 一键精准恢复与 0px 贴边分屏
- **智能实例复用与多开补齐**：支持「缺几个补几个（默认）」「仅复用已打开窗口」「总是新开实例」三种策略；对已有运行实例的浏览器、IDE 与终端自动注入多开参数（如 `--new-window`、`-n`、`-w new`）。
- **启动编排（优先级与延迟）**：支持为每个应用单独设置启动优先级与等待延迟（例如先启动本地代理或数据库服务，延迟 3 秒后再拉起客户端）。
- **像素级无逢贴边**：引入 `DWMWA_EXTENDED_FRAME_BOUNDS` 透明阴影边框自动补偿与 Per-Monitor V2 DPI 换算，消除 Windows 10/11 窗口左右分屏与贴边时的 7~14px 缝隙。
- **专注模式与一键收工**：恢复时可选开启「专注模式」将非本空间窗口安静收起到任务栏（绝不强关软件）；任务结束时可点击「一键收工」向本空间窗口发送标准 `WM_CLOSE` 优雅关闭（有未保存文档时会正常提示保存）。

### 3. 可视化画布与多屏幕拓扑方案（Display Profiles）
- **Visual Workspace Map 画布**：支持鼠标拖拽移动与缩放、边缘磁性吸附、网格辅助线、5 种一键分屏模板（**左右平分 / 左大右小 / 三列均分 / 四宫格 / 居中叠放**）、百分比精确输入以及 30 步 `Ctrl+Z / Ctrl+Y` 撤销重做。
- **多屏幕组合自适应**：同一工作空间可分别为「公司双屏」「笔记本单屏」「居家带鱼屏」保存独立的屏幕拓扑布局；插拔外接显示器或切换环境时，恢复引擎自动匹配当前显示器签名并套用对应布局。

### 4. 资源联动与生命周期钩子（Hooks）
- **文件、目录与书签导入**：支持关联本地文件与文件夹、多行批量粘贴网址，以及一键只读扫描并导入本机 **Chrome / Edge 书签文件夹**（不读取任何密码、Cookie 或历史记录）。
- **生命周期自动化钩子**：支持为工作空间可选配置「一键恢复前」与「一键收工后」静默执行的本机命令（如 `docker start dev-db` / 切换开发环境脚本，默认关闭，外部导入时自动停用保护安全）。

### 5. 历史快照、差异对比与诊断报告
- **多重快照保护**：支持手动快照、后台定时自动快照（带指纹去重与保留上限清理）以及每次恢复/覆盖前的自动保护快照。
- **快照对比与另存**：内置可视化快照预览与双快照差异对比（新增 / 移除 / 位置变动 / 状态变化），支持将任意历史快照一键另存为全新的独立工作空间。
- **逐步恢复诊断**：恢复中心实时展示每个窗口的匹配依据、耗时与像素偏差；对未成功项支持**单条重试**、**仅重试未完成项**、**一键去修复路径**或**一键撤销本次恢复**。

---

## 下载与安装

前往 [GitHub Releases](https://github.com/fanfanhappy/workscene/releases/latest) 下载最新版本（当前正式版 **v1.1.3**）：

| 版本形态 | 文件名 | 适用场景与数据存储说明 |
|---|---|---|
| **标准安装版**（推荐） | `WorkScene_1.1.3_x64-setup.exe` | NSIS 当前用户安装，无需管理员权限，支持桌面/开始菜单快捷方式与标准卸载；数据默认存放在 `%LOCALAPPDATA%\WorkScene`。 |
| **绿色便携版** | `WorkScene-1.1.3-portable.zip` | 免安装解压即用。双击 `start.cmd` 启动时自动启用便携模式，所有数据库与快照均存放在同目录 `data\` 下，拷入 U 盘或同步盘即可随身迁移。 |

> **Windows SmartScreen 提示**：当前开源版本尚未进行商业 EV 代码签名。首次运行安装包或可执行文件时，如遇 Windows SmartScreen 弹出「Windows 已保护你的电脑」，请点击 **「更多信息」→「仍要运行」**。

---

## 快速上手

```mermaid
flowchart LR
    A["1. 摆好当前桌面窗口"] --> B["2. 点击「捕获现场」保存为工作空间"]
    B --> C["3. 在画布微调分屏 / 补充网页与项目目录"]
    C --> D["4. 随时通过主界面、托盘或 Ctrl+Alt+1~9 一键恢复"]
```

1. **首次捕获**：打开你平时工作所需的软件并排好位置，点击右上角 **「捕获现场」**，勾选需要纳入的窗口与自动识别出的项目/文件夹上下文，输入名称保存。
2. **编排与微调**：进入「工作空间」详情页：
   - 在 **窗口摆法** 标签页使用一键分屏模板或拖拽调整比例，按 `Ctrl+S` 保存；如果是多屏办公，可点击「保存为当前屏幕组合布局」；
   - 在 **软件列表** 标签页随时点击「添加软件 (.exe)」补充未打开的程序，或展开高级选项设置启动顺序与延迟；
   - 在 **文件与网址** 标签页添加常用文档、导入浏览器书签夹，或配置恢复前/收工后钩子命令。
3. **日常极速切换**：
   - 按 `Ctrl+Alt+W` 随时唤起主窗口，或按 `Ctrl+K` 打开命令面板搜索并执行操作；
   - 为高频工作空间绑定 `Ctrl+Alt+1 ~ 9` 快捷槽，无需打开主界面即可在后台一键切换整套桌面现场；
   - 右键系统托盘图标，可直接恢复常用工作空间或立即拍下当前桌面快照。

---

## 快捷键速查

| 快捷键 | 作用域 | 功能说明 |
|---|---|---|
| `Ctrl + Alt + W` | 全局（系统级） | 唤起 / 聚焦 WorkScene 主窗口（可在设置中切换为 `Ctrl+Shift+W` 或 `Ctrl+Alt+Space`） |
| `Ctrl + Alt + 1 ~ 9` | 全局（系统级） | 直接在后台恢复对应快捷槽绑定的工作空间（可在设置中开启/关闭或调整绑定） |
| `Ctrl + K` | 应用内全局 | 打开快捷命令面板（搜索工作空间、一键恢复、页面跳转、切换主题） |
| `Ctrl + S` | 工作空间布局页 | 立即保存当前画布的窗口位置与分屏修改 |
| `Ctrl + Z` / `Ctrl + Y` | 工作空间布局页 | 撤销 / 重做画布窗口排版操作（支持 `Ctrl + Shift + Z`） |
| `↑ ↓ ← →` | 工作空间布局页 | 以 `1%` 步进微调选中窗口位置（按住 `Shift + 方向键` 以 `5%` 步进微调） |
| `Esc` | 应用内全局 | 按层级依次关闭当前顶层弹窗、命令面板，或取消画布窗口选中 |

---

## L2 应用适配矩阵

WorkScene 对常见生产力工具内置了二级参数适配（L2 Adapters）与多实例启动优化：

| 类别 | 已适配代表软件 | 适配能力说明 |
|---|---|---|
| **代码编辑器 / AI IDE** | VS Code (`Code.exe`)、Cursor、Windsurf、Trae、Antigravity | 自动识别当前打开的本地工作区目录；多窗口补启时自动注入 `-n` 新窗口标志并传入项目路径。 |
| **JetBrains 系列 IDE** | IntelliJ IDEA、PyCharm、WebStorm、GoLand、CLion、Rider、RustRover | 支持将项目文件夹路径直接生成为启动参数，直接打开目标工程。 |
| **现代浏览器** | Chrome、Microsoft Edge、Brave、Arc、Vivaldi、Firefox | 支持一键只读导入 Chrome / Edge 本地书签夹；多窗口恢复时自动注入 `--new-window` 独立窗口参数。 |
| **终端工具** | Windows Terminal (`wt.exe`)、PowerShell、CMD | 支持为 `wt.exe` 自动生成 `-w new -d "<目录>"` 参数，新开终端窗口并直接进入目标工作目录。 |
| **文件资源管理器** | Windows Explorer (`explorer.exe`) | 捕获时通过 COM 自动采集所有已打开资源管理器窗口的真实文件夹路径，恢复时精准打开对应目录。 |

---

## 隐私承诺与系统能力边界

### 隐私与安全设计
- **100% 本地离线存储**：所有工作空间配置、恢复记录（SQLite WAL 模式）与快照 JSON 文件均存储在本地磁盘，无账号体系、无云端同步、无后台遥测。
- **最小权限与只读采集**：不截取屏幕像素图像、不注入其他进程内存、不读取浏览器 Cookie / 密码库 / 浏览历史。
- **外部导入安全防护**：导入他人分享的工作空间 JSON 时，自动提供导入前预检；对指向 UNC 网络共享路径、不存在路径或含自定义启动参数/钩子命令的条目，默认自动停用或剥离参数，需用户确认后方可启用。
- **唯一的网络请求**：仅当用户主动点击「设置 → 关于 → 检查新版本」按钮时，前端才会向 `api.github.com/repos/fanfanhappy/workscene/releases/latest` 发起一次只读版本查询。

### Windows 系统能力边界
1. **UIPI 管理员权限隔离**：受 Windows 用户界面特权隔离（UIPI）保护，当 WorkScene 以普通用户权限运行时，无法移动或关闭以「管理员身份」运行的窗口（捕获与恢复报告中会明确标注提示）。如需管理提权窗口，请以管理员身份运行 WorkScene。
2. **UWP / 微软商店打包应用**：Windows 统一托管在 `ApplicationFrameWindow` 容器下的 UWP 应用暂不支持独立进程拉起与布局恢复，扫描时会自动识别并归入排除列表。
3. **无标题附属浮层**：尺寸小于 `120×90` 的微型悬浮窗、`WS_EX_TOOLWINDOW` 工具条及系统托盘弹窗会被自动过滤，不纳入工作空间。

---

## 架构与本地开发

### 目录结构

```text
WorkScene/
├── src/                        # React 19 + TypeScript 前端源码
│   ├── components/             # 核心组件（AppShell / LayoutEditor / MonitorCanvas / CommandPalette / OnboardingModal / ui）
│   ├── constants/              # 色标等全局常量定义
│   ├── pages/                  # 六大功能页面（Overview / Workspaces / Capture / Restore / History / Settings）
│   ├── services/               # 唯一 Tauri IPC 通信封装层 (api.ts)
│   ├── stores/                 # Zustand 全局 UI 与主题/弹窗/Toast 状态
│   ├── styles/                 # Tailwind CSS 4 与 Apple 风格深浅色主题变量
│   └── types/                  # 前后端严格对齐的 TypeScript 数据契约 (contract.ts)
├── src-tauri/                  # Rust + Tauri 2 后端源码
│   ├── migrations/             # SQLite 数据库迁移脚本
│   └── src/
│       ├── adapters/           # L2 应用分类与启动参数生成规则
│       ├── capture/            # 窗口快速枚举与 L2 上下文（Explorer / IDE 项目目录）识别
│       ├── commands/           # Tauri IPC 命令入口层
│       ├── core/               # 核心领域模型、错误类型与时间戳工具
│       ├── data/               # SQLite 数据访问层（workspaces / snapshots / runs / settings / extras）
│       ├── restore/            # 恢复引擎、多实例最近邻匹配器 (matcher) 与比例坐标换算 (layout)
│       ├── storage/            # 原子化 JSON 快照读写与文件健康度校验
│       └── win/                # Win32 / DWM / DPI / 热键 / 托盘 / 图标提取 / 书签解析 / 开机自启封装
├── docs/                       # 版本发布说明与架构文档 (RELEASE.md)
└── .github/workflows/          # GitHub Actions 自动化测试、NSIS 打包与 Release 发布流水线
```

### 本地开发环境要求
- **操作系统**：Windows 10 / 11 (64-bit)
- **前端工具链**：Node.js 20+、`pnpm` 9+
- **后端工具链**：Rust 1.77+ (`stable-x86_64-pc-windows-msvc`)、Visual Studio C++ Build Tools、WebView2 Runtime

### 常用开发命令

```powershell
# 1. 安装前端依赖
pnpm install

# 2. 启动开发调试模式（带热更新）
pnpm tauri dev

# 3. 运行 Rust 核心单元测试（69+ 单测）与前端类型/生产构建检查
cd src-tauri; cargo test --lib; cd ..
pnpm build

# 4. 构建生产级可执行文件与 NSIS 安装包
#    产物路径：src-tauri/target/release/bundle/nsis/WorkScene_<version>_x64-setup.exe
pnpm tauri build
```

---

## 常见问题（FAQ）

<details>
<summary><b>Q1：开启「随 Windows 开机启动」时提示注册表或启动文件夹被拦截怎么办？</b></summary>

部分安全软件（如 Windows Defender 文件夹限制访问、360、火绒等）可能会拦截未签名程序写入 `HKCU\...\Run` 注册表或系统「启动」文件夹。WorkScene 已内置「注册表 → 启动文件夹 `.vbs` 脚本」双通道自动降级；若均被安全软件拦截，设置页会直接弹出提示并提供 **「打开启动文件夹」** 按钮，点击后将 `WorkScene.exe` 的快捷方式手动拖入该目录即可。
</details>

<details>
<summary><b>Q2：换电脑或重装系统时，如何迁移我的所有工作空间？</b></summary>

有两种方式：
1. 打开 **「设置 → 存储与目录」**，点击 **「导出全部备份」**，会将所有工作空间、窗口布局、多屏方案、资源与生命周期钩子打包为单个 `.json` 文件，在新电脑上点击 **「导入备份」** 即可一键还原；
2. 若使用 `WorkScene-1.1.3-portable.zip` 便携版并通过 `start.cmd` 启动，直接把整个文件夹复制到新电脑即可。
</details>

<details>
<summary><b>Q3：笔记本外接显示器拔掉后，原来在副屏的窗口恢复时会跑丢吗？</b></summary>

不会。当目标显示器不存在时，恢复引擎会自动将原本位于副屏的窗口按比例安全回退到当前主显示器的工作区内，并在恢复报告中给出显示器变更说明；你也可以为「单屏」和「双屏」分别保存专属的拓扑布局方案。
</details>

---

## 发布记录

完整的版本演进与更新日志请参阅 [docs/RELEASE.md](docs/RELEASE.md) 或 [GitHub Releases](https://github.com/fanfanhappy/workscene/releases)。
