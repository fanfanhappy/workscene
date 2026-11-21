# ADR-0003：M2 采用「关系数据入库 + 快照 payload 留文件」的混合存储

- 状态：已采纳（M2，2026-11-21）
- 关联：ADR-0001、PRD §8、评审补充意见 A1/A3

## 背景

M2 引入 SQLite（`workscene.db`）承载工作空间模型。这里有一个选择：把快照内容（每个窗口的标题、类名、矩形、DPI）也拆成 `snapshot_items` / `snapshot_windows` 行，还是让快照继续是一份不可变 JSON。

## 决策

**元数据入库，payload 留文件。**

- 入库（可查询、有关系、可聚合）：`workspaces`、`workspace_items`、`app_configs`、`window_layouts`、`snapshots`（索引行 + `payload_path` + `content_fingerprint`）、`restore_runs`、`restore_steps`、`app_settings`。
- 留文件（不可变、整体读写）：`snapshots/scene-<id>.json`，一份完整 `SceneFile`。

恢复时的数据流向：

```
workspace 当前布局（window_layouts + workspace_items）
        └─ to_scene() ─→ SceneFile ─→ restore::apply()
snapshot payload（JSON 文件）
        └─ load_scene() ─→ SceneFile ─→ restore::apply()   （撤销 / 历史回退）
```

两条路径汇到同一个恢复引擎，引擎不感知数据来自 DB 还是文件。

## 理由

1. **快照是不可变整体**：从文件整体读取、整体比较（指纹）、整体丢弃，比 7 张表 join 重建更简单，也不会出现"快照写了一半"的半行状态。
2. **恢复引擎已经吃 `SceneFile`**：M1 的实现与测试都围绕它，改表结构不带来任何能力提升。
3. **历史与撤销不需要跨快照查询**：真正跨快照的查询（按错误码聚合、按时间线）发生在 `restore_*` 表，那些已经入库。
4. **代价可控**：`payload_path` 让 DB 仍然是索引真源，文件只是内容载体；删除工作空间时按 `payload_path` 一并清理（`workspaces::delete`）。

## 后果与约束

- **一致性**：DB 插入与文件写入不在同一事务里。当前做法是先写文件、再插 DB 行；若插库失败会留下孤儿文件，`scene_health` 能发现，但不会自动清理（M3 加清理任务）。
- **不可查询**：不能按"包含 Chrome 的所有快照"做 SQL 查询，只能遍历 payload 或退化为按工作空间查。目前没有这个需求。
- **体积**：快照 JSON 单文件约 5–10 KB（9 窗口），可接受；上千快照时需要保留策略（M5）。
- **禁止**：不得在 DB 里再存一份窗口坐标，否则两份真源必然漂移。

## 被拒绝的替代方案

- **全部入库**：12 张表 + 大 JSON 字段（`payload_json`），查询能力没增加，却把"整体读写"拆成"跨表事务"，还引入 payload 版本迁移问题。
- **全部留文件**：M2 的工作空间需要关系（items ↔ layouts ↔ snapshots ↔ runs），纯文件做不好级联与统计。
