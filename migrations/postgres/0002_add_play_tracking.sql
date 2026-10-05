ALTER TABLE tracks ADD COLUMN IF NOT EXISTS last_played_at TIMESTAMPTZ;
ALTER TABLE tracks ADD COLUMN IF NOT EXISTS play_count     INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_track_last_played ON tracks(last_played_at);