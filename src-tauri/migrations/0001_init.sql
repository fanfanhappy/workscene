-- M2 初始架构：按评审意见收敛的最小表集（不是 PRD 的 12 张表）
-- 关联：docs/adr/0001-json-snapshots-instead-of-sqlite.md

PRAGMA foreign_keys = ON;

CREATE TABLE workspaces (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    description           TEXT,
    color                 TEXT,
    icon                  TEXT,
    is_favorite           INTEGER NOT NULL DEFAULT 0,
    sort_order            INTEGER NOT NULL DEFAULT 0,
    auto_snapshot_enabled INTEGER NOT NULL DEFAULT 1,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    last_restored_at      INTEGER
);
CREATE INDEX idx_workspaces_sort ON workspaces(sort_order, name);

CREATE TABLE workspace_items (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    item_type    TEXT NOT NULL,
    display_name TEXT NOT NULL,
    -- 稳定身份：可执行文件路径（不可读时为进程名），用于去重与重新定位
    identity_key TEXT NOT NULL,
    target       TEXT,
    target_kind  TEXT NOT NULL DEFAULT 'path',
    enabled      INTEGER NOT NULL DEFAULT 1,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE (workspace_id, item_type, identity_key)
);
CREATE INDEX idx_items_workspace ON workspace_items(workspace_id);

CREATE TABLE app_configs (
    item_id             TEXT PRIMARY KEY REFERENCES workspace_items(id) ON DELETE CASCADE,
    launch_args_json    TEXT,
    working_directory   TEXT,
    reuse_policy        TEXT NOT NULL DEFAULT 'reuse_or_launch',
    wait_timeout_ms     INTEGER NOT NULL DEFAULT 8000,
    window_match_json   TEXT,
    adapter_config_json TEXT
);

CREATE TABLE window_layouts (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    item_id       TEXT NOT NULL REFERENCES workspace_items(id) ON DELETE CASCADE,
    monitor_key   TEXT NOT NULL,
    x_ratio       REAL NOT NULL,
    y_ratio       REAL NOT NULL,
    width_ratio   REAL NOT NULL,
    height_ratio  REAL NOT NULL,
    original_x    INTEGER,
    original_y    INTEGER,
    original_width  INTEGER,
    original_height INTEGER,
    source_dpi    INTEGER,
    show_state    TEXT NOT NULL DEFAULT 'normal',
    z_order       INTEGER NOT NULL DEFAULT 0,
    title         TEXT NOT NULL DEFAULT '',
    class_name    TEXT NOT NULL DEFAULT '',
    window_match_json TEXT,
    captured_at   INTEGER NOT NULL
);
CREATE INDEX idx_layouts_workspace ON window_layouts(workspace_id);
CREATE INDEX idx_layouts_item ON window_layouts(workspace_id, item_id);

CREATE TABLE snapshots (
    id                TEXT PRIMARY KEY,
    workspace_id      TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name              TEXT NOT NULL,
    snapshot_type     TEXT NOT NULL,
    trigger_type      TEXT NOT NULL,
    captured_at       INTEGER NOT NULL,
    window_count      INTEGER NOT NULL DEFAULT 0,
    display_count     INTEGER NOT NULL DEFAULT 0,
    -- 快照内容仍是 JSON 文件（ADR-0001），DB 只存索引与可查询元数据
    payload_path      TEXT NOT NULL,
    content_fingerprint TEXT,
    payload_version   INTEGER NOT NULL DEFAULT 1,
    parent_snapshot_id TEXT REFERENCES snapshots(id) ON DELETE SET NULL,
    -- 保护快照不参与自动淘汰（评审 R6）
    is_protected      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_snapshots_workspace ON snapshots(workspace_id, captured_at DESC);
CREATE INDEX idx_snapshots_fingerprint ON snapshots(workspace_id, content_fingerprint);

CREATE TABLE restore_runs (
    id             TEXT PRIMARY KEY,
    workspace_id   TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
    snapshot_id    TEXT REFERENCES snapshots(id) ON DELETE SET NULL,
    status         TEXT NOT NULL,
    trigger_type   TEXT NOT NULL DEFAULT 'manual',
    started_at     INTEGER NOT NULL,
    finished_at    INTEGER,
    total_steps    INTEGER NOT NULL DEFAULT 0,
    succeeded      INTEGER NOT NULL DEFAULT 0,
    failed         INTEGER NOT NULL DEFAULT 0,
    skipped        INTEGER NOT NULL DEFAULT 0,
    error_summary  TEXT
);
CREATE INDEX idx_runs_started ON restore_runs(started_at DESC);

CREATE TABLE restore_steps (
    id             TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL REFERENCES restore_runs(id) ON DELETE CASCADE,
    seq            INTEGER NOT NULL,
    step_type      TEXT NOT NULL,
    target_name    TEXT NOT NULL,
    status         TEXT NOT NULL,
    -- 机器可读错误码（见 docs/RESTORE_CONTRACT.md）
    error_code     TEXT,
    message        TEXT NOT NULL,
    match_note     TEXT,
    duration_ms    INTEGER,
    delta_x        INTEGER,
    delta_y        INTEGER,
    state_restored INTEGER NOT NULL DEFAULT 0,
    UNIQUE (run_id, seq)
);
CREATE INDEX idx_steps_run ON restore_steps(run_id, seq);

CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
