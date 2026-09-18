-- Secondary indexes for performance, fast timeline pagination, and foreign key cascade protection

-- 1. Accelerates get_by_chat and prevents full-table scan on chat DELETE CASCADE
CREATE INDEX IF NOT EXISTS idx_messages_chat_timestamp 
ON messages (session_id, chat_jid, timestamp DESC);

-- 2. Accelerates global session timeline reads
CREATE INDEX IF NOT EXISTS idx_messages_session_timestamp 
ON messages (session_id, timestamp DESC);

-- 3. Accelerates media cache reference lookups and orphan pruning
CREATE INDEX IF NOT EXISTS idx_messages_media_sha256 
ON messages (media_sha256) WHERE media_sha256 IS NOT NULL;

-- 4. Accelerates chats list sorted by latest activity
CREATE INDEX IF NOT EXISTS idx_chats_session_last_msg 
ON chats (session_id, last_message_at DESC);

-- 5. Accelerates contact book queries by display name
CREATE INDEX IF NOT EXISTS idx_contacts_session_name 
ON contacts (session_id, push_name ASC, jid ASC);

-- 6. Accelerates participant lookups across groups
CREATE INDEX IF NOT EXISTS idx_group_participants_user 
ON group_participants (session_id, user_jid);

-- 7. Accelerates calls queries by session
CREATE INDEX IF NOT EXISTS idx_calls_session_created 
ON calls (session_id, created_at DESC);

-- 8. Supports LRU eviction queries for disk cache management
CREATE INDEX IF NOT EXISTS idx_media_cache_access 
ON media_cache (last_accessed_at ASC);

