ALTER TABLE tracks RENAME COLUMN thumbnail_url TO thumbnail_small;
ALTER TABLE tracks ADD COLUMN IF NOT EXISTS thumbnail_large TEXT;