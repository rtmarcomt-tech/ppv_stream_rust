-- Photos reuse existing purchases and allowlist by retaining the videos row identifier.
ALTER TABLE videos ADD COLUMN IF NOT EXISTS media_type TEXT NOT NULL DEFAULT 'video';
ALTER TABLE videos DROP CONSTRAINT IF EXISTS videos_media_type_check;
ALTER TABLE videos ADD CONSTRAINT videos_media_type_check CHECK (media_type IN ('video', 'photo'));
CREATE INDEX IF NOT EXISTS idx_videos_media_type_created_at ON videos (media_type, created_at DESC);
