# ADR-0001：M1 用 JSON 文件而不是 SQLite

- 状态：已采纳（M1，2026-11-21）
- 决策者：开发
- 关联：PRD §8（数据库设计）、评审补充意见 A1

## 背景

PRD v1.0 §8 定义了 SQLite + SQLx 的 12 张表（`workspaces`、`workspace_items`、`app_definitions`、`app_configs`、`window_layouts`、`display_profiles`、`snapshots`、`snapshot_items`、`snapshot_windows`、`snapshot_displays`、`restore_runs`、`restore_steps`、`audit_events`）。

M1 的目标是验证「捕获 → 保存 → 重启 → 恢复」这条闭环是否在真实 Windows 上成立。此时**唯一不确定的是窗口识别与坐标映射**，而工作空间建模、关系约束、迁移这些数据库价值，在闭环成立之前都只是成本。

## 决策

M1 阶段快照存为单文件 JSON：`%LOCALAPPDATA%\WorkScene\snapshots\scene-<id>.json`，结构为 `SceneFile { schemaVersion, id, name, capturedAt, displays, windows }`。

SQLite 延后到 M2，且**不照搬 12 张表**：进入 M2 时以评审修正后的最小表集重建（`workspaces` / `workspace_items` / `snapshots` / `snapshot_windows` / `restore_runs` / `restore_steps`），并按 PRD 评审第 3 节补齐约束与索引。

## 后果

**得到**

- M1 无需迁移系统即可验证核心能力；
- 快照可读、可 diff、可手工构造测试数据；
- 报告与快照落盘复用同一套原子写逻辑。

**付出（必须接受的代价）**

- `SceneFile.schema_version` 是裸整数，只有「大于当前版本就拒绝」，**没有迁移函数**；
- 没有外键与级联：删除工作空间不会级联清理快照（M1 尚无工作空间概念，风险延后）；
- 无跨文件事务：跨多个快照的操作（如批量清理）不原子；
- 无索引：`list_scenes` 全目录扫描，快照数上千后需要重设计。

**落地要求（已实现）**

- 写入走 `tmp → bak → rename`，避免半个 JSON（补充意见 A3）；
- 解析失败的文件不静默丢弃，由 `scene_health` 报给 UI；
- `scene_save` 的 id 只接受内部白名单格式，防路径穿越。

## 被拒绝的替代方案

- **直接上 SQLite**：M1 期间每次调参都要写迁移，闭环验证成本翻倍。
- **JSON + 内存索引常驻**：进程重启后状态不一致，且与「快照不可变」冲突。
