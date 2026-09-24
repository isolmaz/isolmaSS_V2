-- @@
CREATE TABLE IF NOT EXISTS preferences (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  max_active INTEGER NOT NULL DEFAULT 50 CHECK (max_active BETWEEN 1 AND 100000),
  max_image_bytes INTEGER NOT NULL DEFAULT 10485760 CHECK (max_image_bytes BETWEEN 1024 AND 104857600),
  max_storage_bytes INTEGER NOT NULL DEFAULT 1000000000 CHECK (max_storage_bytes BETWEEN 1048576 AND 1000000000000),
  daily_upload_limit INTEGER NOT NULL DEFAULT 20 CHECK (daily_upload_limit BETWEEN 1 AND 100000),
  daily_view_limit INTEGER NOT NULL DEFAULT 1000 CHECK (daily_view_limit BETWEEN 1 AND 100000000),
  retention_days INTEGER NOT NULL DEFAULT 30 CHECK (retention_days BETWEEN 1 AND 3650),
  warning_percent INTEGER NOT NULL DEFAULT 90 CHECK (warning_percent BETWEEN 1 AND 100),
  limit_action TEXT NOT NULL DEFAULT 'warn' CHECK (limit_action IN ('warn','block_upload','block_all'))
);
-- @@
INSERT OR IGNORE INTO preferences(id) VALUES (1);

-- @@
CREATE TABLE IF NOT EXISTS images (
  id TEXT PRIMARY KEY,
  object_key TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL CHECK (state IN ('pending','active','deleting')),
  content_type TEXT NOT NULL CHECK (content_type IN ('image/png','image/jpeg')),
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  deleted_at INTEGER,
  password_salt TEXT,
  password_hash TEXT,
  views INTEGER NOT NULL DEFAULT 0
);
-- @@
CREATE INDEX IF NOT EXISTS images_active_created ON images(state, created_at, id);
-- @@
CREATE INDEX IF NOT EXISTS images_expires ON images(state, expires_at);

-- @@
CREATE TABLE IF NOT EXISTS usage_daily (
  day TEXT PRIMARY KEY,
  uploads INTEGER NOT NULL DEFAULT 0 CHECK (uploads >= 0),
  views INTEGER NOT NULL DEFAULT 0 CHECK (views >= 0),
  bytes_uploaded INTEGER NOT NULL DEFAULT 0 CHECK (bytes_uploaded >= 0)
);
-- @@
CREATE TRIGGER IF NOT EXISTS upload_quota_insert BEFORE INSERT ON usage_daily
WHEN (SELECT limit_action FROM preferences WHERE id = 1) != 'warn'
AND (NEW.uploads > MAX(1, (SELECT daily_upload_limit * warning_percent / 100 FROM preferences WHERE id = 1))
  OR ((SELECT limit_action FROM preferences WHERE id = 1) = 'block_all'
    AND NEW.views >= MAX(1, (SELECT daily_view_limit * warning_percent / 100 FROM preferences WHERE id = 1)))
  OR ((SELECT limit_action FROM preferences WHERE id = 1) = 'block_all'
    AND (SELECT COALESCE(SUM(size_bytes), 0) FROM images WHERE state = 'active')
      >= (SELECT max_storage_bytes * warning_percent / 100 FROM preferences WHERE id = 1)))
BEGIN SELECT RAISE(ABORT, 'quota_uploads'); END;
-- @@
CREATE TRIGGER IF NOT EXISTS upload_quota_update BEFORE UPDATE OF uploads ON usage_daily
WHEN (SELECT limit_action FROM preferences WHERE id = 1) != 'warn'
AND (NEW.uploads > MAX(1, (SELECT daily_upload_limit * warning_percent / 100 FROM preferences WHERE id = 1))
  OR ((SELECT limit_action FROM preferences WHERE id = 1) = 'block_all'
    AND NEW.views >= MAX(1, (SELECT daily_view_limit * warning_percent / 100 FROM preferences WHERE id = 1)))
  OR ((SELECT limit_action FROM preferences WHERE id = 1) = 'block_all'
    AND (SELECT COALESCE(SUM(size_bytes), 0) FROM images WHERE state = 'active')
      >= (SELECT max_storage_bytes * warning_percent / 100 FROM preferences WHERE id = 1)))
BEGIN SELECT RAISE(ABORT, 'quota_uploads'); END;
-- @@
CREATE TRIGGER IF NOT EXISTS view_quota_insert BEFORE INSERT ON usage_daily
WHEN (SELECT limit_action FROM preferences WHERE id = 1) = 'block_all'
AND (NEW.views > MAX(1, (SELECT daily_view_limit * warning_percent / 100 FROM preferences WHERE id = 1))
  OR NEW.uploads >= MAX(1, (SELECT daily_upload_limit * warning_percent / 100 FROM preferences WHERE id = 1))
  OR (SELECT COALESCE(SUM(size_bytes), 0) FROM images WHERE state = 'active')
    >= (SELECT max_storage_bytes * warning_percent / 100 FROM preferences WHERE id = 1))
BEGIN SELECT RAISE(ABORT, 'quota_views'); END;
-- @@
CREATE TRIGGER IF NOT EXISTS view_quota_update BEFORE UPDATE OF views ON usage_daily
WHEN (SELECT limit_action FROM preferences WHERE id = 1) = 'block_all'
AND (NEW.views > MAX(1, (SELECT daily_view_limit * warning_percent / 100 FROM preferences WHERE id = 1))
  OR NEW.uploads >= MAX(1, (SELECT daily_upload_limit * warning_percent / 100 FROM preferences WHERE id = 1))
  OR (SELECT COALESCE(SUM(size_bytes), 0) FROM images WHERE state = 'active')
    >= (SELECT max_storage_bytes * warning_percent / 100 FROM preferences WHERE id = 1))
BEGIN SELECT RAISE(ABORT, 'quota_views'); END;
-- @@
CREATE TRIGGER IF NOT EXISTS storage_quota BEFORE UPDATE OF state ON images
WHEN NEW.state = 'active' AND (SELECT limit_action FROM preferences WHERE id = 1) != 'warn'
AND (SELECT COALESCE(SUM(size_bytes), 0) FROM images WHERE state = 'active') + NEW.size_bytes
 > (SELECT max_storage_bytes * warning_percent / 100 FROM preferences WHERE id = 1)
BEGIN SELECT RAISE(ABORT, 'quota_storage'); END;

-- @@
CREATE TABLE IF NOT EXISTS password_attempts (
  id TEXT NOT NULL,
  hour INTEGER NOT NULL,
  failures INTEGER NOT NULL,
  PRIMARY KEY(id, hour)
);
