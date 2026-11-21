# 恢复契约（v0.1，部分实现）

- 关联：评审补充意见 A4、PRD §4.5 恢复任务状态机、UI 规范 §9 三态设计
- 状态：**草案。表中「已实现」列为 No 的项目尚未落地，不要按已支持宣传。**

一个恢复任务由后端产出 `RestoreReport`，前端只展示不推导。本文件是后端、UI 文案、验收脚本三方唯一的交点。

## 1. 谁算成功

| 情况 | 判定 |
|---|---|
| 位置 + 尺寸偏差 ≤ `max(8px, 2×scale)`，且显示状态与快照一致 | `success` |
| 位置正确但最大化未生效 | `failed`（`message` 明确说"未能保持最大化"） |
| 快照为最小化，已还原成普通窗口 | `success` + `stateRestored=false` + 文案说明 |
| 匹配歧义 / 无候选 / 应用未运行 | `skipped`（绝不静默改别的窗口） |
| `SetWindowPlacement` 返回失败 | `failed` |
| 应用自身把窗口改回原点（二次校正后仍超阈值） | `failed` + 实际偏差值 |

`stateRestored` 存在的唯一目的：让"位置对了但状态没还原"这种半成功不被算成完全成功。

## 2. 步骤状态与文案（当前实现）

`StepStatus` 只有 5 个值：`success` / `failed` / `skipped` / `timeout` / `needsReview`（`needsReview` 目前只在预览里用）。

| 场景 | status | message（当前中文文案） | 已实现 |
|---|---|---|---|
| 正常恢复 | success | `已恢复，位置/尺寸偏差 Npx` | Yes |
| 最大化恢复 | success | `已最大化到目标显示器` | Yes |
| 最大化失败 | failed | `位置已恢复，但窗口未能保持最大化` | Yes |
| 最小化被还原为普通 | success | `已恢复到目标位置；原快照中该窗口为最小化…` | Yes |
| 漂移 | failed | `窗口被应用自身改回原位置（偏差 Npx，阈值 Mpx）…` | Yes |
| 写入失败 | failed | `SetWindowPlacement 调用失败（窗口已关闭或受 UIPI 限制）` | Yes |
| 歧义 | skipped | `窗口匹配存在歧义，已跳过（不改动任何窗口）` | Yes |
| 无窗口 | skipped | `未找到可用的对应窗口…` | Yes |
| 显示器缺失 | — | 走 `displayNotes`，位置映射到主屏 | Yes |

## 3. error_code（v0.2 已部分落地）

`RestoreStepResult.code` 与 `restore_steps.error_code` 已开始写入，`run_error_stats` 可按 code 聚合。

| error_code | 含义 | 建议动作 | 状态 |
|---|---|---|---|
| `match_ambiguous` | 多候选评分接近 | 打开候选选择器 | **已实现** |
| `window_not_found` | 无匹配窗口 | 启动应用 / 重新捕获 | **已实现** |
| `window_control_denied` | 写入位置失败（已关闭或 UIPI 限制） | 以相同权限运行 | **已实现** |
| `layout_drift` | 应用改回自身位置 | 查看建议 | **已实现** |
| `state_not_restored` | 最大化/最小化未还原 | 手动最大化 | **已实现** |
| `exe_missing` | 可执行文件不存在 | 重新定位应用 | 未实现 |
| `launch_timeout` | 启动后超时未见窗口 | 重试 | 未实现（当前归入 `window_not_found`） |
| `display_missing` | 目标显示器不在 | 查看映射预览 | 部分（仅 `displayNotes`） |
| `resource_open_failed` | 文件/文件夹/网址打不开 | 检查路径或关联程序 | **已实现（M9）** |
| `uipi_denied` | 权限不足 | 提示提权 | 未实现（与 `window_control_denied` 合并） |

## 4. 报告落盘

每次恢复写 `%LOCALAPPDATA%\WorkScene\runs\<runId>.json`（原子写）。`run_list` 可回查最近 N 次；UI 目前还没有"最近恢复记录"页面（M2）。

## 5. 保护快照

`restore_apply` 在开始前自动把当前桌面存为「恢复前自动保护」，并把 id 放进 `report.preRestoreSceneId`。UI 的报告页据此提供「撤销这次恢复」（对该快照再执行一次恢复）。

注意：保护快照目前**没有**独立的保留策略标记，会和普通快照一样出现在列表里，自动清理将来落地时必须把它排除在外（PRD 评审 R6）。
