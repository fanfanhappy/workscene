-- 启动优先级与列表排序分离。
--
-- 之前 launch_priority 直接复用 sort_order：取消“优先启动”会把应用
-- 置到列表最前，而不是恢复它原来的位置；多个优先应用也共用 -10。
-- 新增独立列，旧数据把原来的 sort_order 值回填为优先级，保持原有启动顺序。
ALTER TABLE workspace_items ADD COLUMN launch_priority INTEGER NOT NULL DEFAULT 0;

UPDATE workspace_items
SET launch_priority = sort_order
WHERE item_type = 'application';
