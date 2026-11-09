-- The historical outbox migration is empty. Keep it immutable because SQLx
-- records migration checksums, and repair missing objects additively here.
CREATE TABLE IF NOT EXISTS outbox_events (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    aggregate_id TEXT,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING',
    retry_count INTEGER NOT NULL DEFAULT 0,
    next_retry_at INTEGER NOT NULL DEFAULT 0,
    lease_until INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_outbox_poll
    ON outbox_events (status, next_retry_at);
CREATE INDEX IF NOT EXISTS idx_outbox_aggregate_seq
    ON outbox_events (session_id, aggregate_id, created_at);

CREATE TABLE IF NOT EXISTS webhook_dlq (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    aggregate_id TEXT,
    payload TEXT NOT NULL,
    retry_count INTEGER NOT NULL,
    error_reason TEXT NOT NULL,
    failed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
