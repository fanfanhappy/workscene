-- 工作空间快捷槽：Ctrl+Alt+1..9 固定绑定到具体工作空间。
--
-- 之前是按"列表里第几个"临时推算，删除/新增工作空间后同一个快捷键会指向
-- 另一个空间（用户以为快捷键坏了，实际是绑定漂移）。改为持久化绑定。
ALTER TABLE workspaces ADD COLUMN hotkey_slot INTEGER;

-- 一个槽只能绑一个工作空间；NULL 表示未绑定（SQLite 的唯一索引忽略 NULL）
CREATE UNIQUE INDEX IF NOT EXISTS idx_workspaces_hotkey_slot
    ON workspaces (hotkey_slot)
    WHERE hotkey_slot IS NOT NULL;
