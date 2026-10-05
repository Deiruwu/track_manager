-- ARTISTS
CREATE TABLE IF NOT EXISTS artists (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL
);

-- ALBUMS
CREATE TABLE IF NOT EXISTS albums (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL
);

-- TRACKS
CREATE TABLE IF NOT EXISTS tracks (
    uuid             TEXT PRIMARY KEY,
    title            TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    album_id         TEXT REFERENCES albums(id) ON DELETE SET NULL,
    thumbnail_small  TEXT,
    thumbnail_large  TEXT,
    bpm              INTEGER,
    camelot_key      TEXT,
    file_path        TEXT,
    added_at         TEXT DEFAULT CURRENT_TIMESTAMP,
    last_played_at   TEXT,
    play_count       INTEGER NOT NULL DEFAULT 0
);

-- RELACIÓN TRACKS -> ARTISTS
CREATE TABLE IF NOT EXISTS track_artists (
    track_uuid TEXT REFERENCES tracks(uuid) ON DELETE CASCADE,
    artist_id  TEXT REFERENCES artists(id)  ON DELETE CASCADE,
    PRIMARY KEY (track_uuid, artist_id)
);

-- ÍNDICES
CREATE INDEX IF NOT EXISTS idx_track_album       ON tracks(album_id);
CREATE INDEX IF NOT EXISTS idx_track_artist      ON track_artists(artist_id);
CREATE INDEX IF NOT EXISTS idx_track_added_at    ON tracks(added_at);
CREATE INDEX IF NOT EXISTS idx_track_last_played ON tracks(last_played_at);
