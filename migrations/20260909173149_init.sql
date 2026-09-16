CREATE TABLE IF NOT EXISTS "sessions" (
	"id" TEXT NOT NULL,
	"phone_number" TEXT,
	"push_name" TEXT,
	"status" TEXT NOT NULL DEFAULT 'QRCODE',
	"qr_payload" TEXT,
	"created_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	"updated_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "chats" (
	"session_id" TEXT NOT NULL,
	"jid" TEXT NOT NULL,
	"name" TEXT,
	-- 'direct', 'group', 'newsletter' ou 'broadcast'
	"kind" TEXT NOT NULL DEFAULT 'direct',
	"unread_count" INTEGER NOT NULL DEFAULT 0,
	"is_archived" BOOLEAN NOT NULL DEFAULT 0,
	"is_pinned" BOOLEAN NOT NULL DEFAULT 0,
	"muted_until" INTEGER,
	"last_message_at" INTEGER,
	"subject_owner" TEXT,
	"subject_time" INTEGER,
	"created_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	"updated_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	PRIMARY KEY("session_id", "jid"),
	FOREIGN KEY ("session_id") REFERENCES "sessions"("id")
	ON UPDATE NO ACTION ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "media_cache" (
	"sha256" TEXT,
	"disk_path" TEXT NOT NULL,
	"mime_type" TEXT NOT NULL,
	"file_size" INTEGER NOT NULL,
	"ref_count" INTEGER NOT NULL DEFAULT 1,
	"last_accessed_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	"created_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	PRIMARY KEY("sha256")
);

CREATE TABLE IF NOT EXISTS "messages" (
	"session_id" TEXT NOT NULL,
	"id" TEXT NOT NULL,
	"chat_jid" TEXT NOT NULL,
	"sender_jid" TEXT NOT NULL,
	-- ID da mensagem respondida (se houver)
	"quoted_message_id" TEXT,
	"is_from_me" BOOLEAN NOT NULL,
	-- 'text', 'image', 'audio', 'sticker', etc.
	"message_type" TEXT NOT NULL,
	"body" TEXT,
	"cdn_url" TEXT,
	"media_key" BLOB,
	"file_enc_sha256" TEXT,
	"media_sha256" TEXT,
	-- 'pending', 'sent', 'delivered', 'read'.
	"delivery_status" TEXT NOT NULL,
	"is_edited" BOOLEAN NOT NULL DEFAULT 0,
	"is_deleted" BOOLEAN NOT NULL DEFAULT 0,
	"timestamp" TIMESTAMP NOT NULL,
	PRIMARY KEY("id", "session_id"),
	FOREIGN KEY ("session_id") REFERENCES "sessions"("id")
	ON UPDATE NO ACTION ON DELETE CASCADE,
	FOREIGN KEY ("session_id", "chat_jid") REFERENCES "chats"("session_id", "jid")
	ON UPDATE NO ACTION ON DELETE CASCADE,
	FOREIGN KEY ("media_sha256") REFERENCES "media_cache"("sha256")
	ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS "contacts" (
	"session_id" TEXT NOT NULL,
	"jid" TEXT NOT NULL,
	"lid_jid" TEXT,
	"phone_number" TEXT,
	"push_name" TEXT,
	"status_text" TEXT,
	"profile_pic_url" TEXT,
	"is_business" BOOLEAN NOT NULL DEFAULT 0,
	"username" TEXT,
	"devices" TEXT,
	"verified_name" TEXT,
	"updated_at" TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	PRIMARY KEY("session_id", "jid"),
	FOREIGN KEY ("session_id") REFERENCES "sessions"("id")
	ON UPDATE NO ACTION ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "group_participants" (
	"session_id" TEXT NOT NULL,
	"group_jid" TEXT NOT NULL,
	"user_jid" TEXT NOT NULL,
	-- 'member', 'admin', 'superadmin'.
	"role" TEXT NOT NULL DEFAULT 'member',
	"joined_at" TIMESTAMP NOT NULL,
	PRIMARY KEY("session_id", "group_jid", "user_jid"),
	FOREIGN KEY ("session_id", "group_jid") REFERENCES "chats"("session_id", "jid")
	ON UPDATE NO ACTION ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "calls" (
	"session_id" TEXT NOT NULL,
	"id" TEXT NOT NULL,
	"chat_jid" TEXT NOT NULL,
	"caller_jid" TEXT NOT NULL,
	"is_video" BOOLEAN NOT NULL DEFAULT 0,
	"is_group_call" BOOLEAN NOT NULL DEFAULT 0,
	-- 'offered', 'accepted', 'rejected', 'ended', 'missed'.
	"status" TEXT NOT NULL,
	"duration_seconds" INTEGER,
	"created_at" TIMESTAMP NOT NULL,
	"ended_at" TIMESTAMP,
	PRIMARY KEY("id", "session_id"),
	FOREIGN KEY ("session_id") REFERENCES "sessions"("id")
	ON UPDATE NO ACTION ON DELETE CASCADE
);
