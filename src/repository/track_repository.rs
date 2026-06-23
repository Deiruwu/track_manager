use sqlx::PgPool;
use tracing::info;
use crate::model::{Album, Artist, Track};
use crate::repository::errors::RepositoryError;

#[derive(Clone)]
pub struct TrackRepository {
    pool: PgPool,
}

impl TrackRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ── Lectura ───────────────────────────────────────────────────────────────

    /// Busca un track por su ID, resolviendo álbum y artistas en el mismo viaje.
    /// Devuelve `None` si no existe.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<Track>, RepositoryError> {
        let row = sqlx::query!(
            r#"
            SELECT
                t.uuid,
                t.title,
                t.duration_seconds,
                t.thumbnail_small,
                t.thumbnail_large,
                t.bpm,
                t.camelot_key,
                t.file_path,
                t.added_at,
                al.id   AS "album_id?",
                al.name AS "album_name?"
            FROM   tracks t
            LEFT JOIN albums al ON al.id = t.album_id
            WHERE  t.uuid = $1
            "#,
            id
        )
            .fetch_optional(&self.pool)
            .await?;

        let row = match row {
            Some(r) => r,
            None    => return Ok(None),
        };

        let artists = self.get_artists_for_track(id).await?;

        let album = match (row.album_id, row.album_name) {
            (Some(id), Some(name)) => Some(Album { id, name }),
            _                      => None,
        };

        Ok(Some(Track {
            id:               row.uuid,
            title:            row.title,
            duration_seconds: row.duration_seconds,
            thumbnail_small:  row.thumbnail_small,
            thumbnail_large:  row.thumbnail_large,
            bpm:              row.bpm,
            camelot_key:      row.camelot_key,
            file_path:        row.file_path,
            added_at:         row.added_at,
            album,
            artists,
        }))
    }

    pub async fn get_all(&self) -> Result<Vec<Track>, RepositoryError> {
        let rows = sqlx::query!(
        r#"
        SELECT
            t.uuid,
            t.title,
            t.duration_seconds,
            t.thumbnail_small,
            t.thumbnail_large,
            t.bpm,
            t.camelot_key,
            t.file_path,
            t.added_at,
            al.id   AS "album_id?",
            al.name AS "album_name?"
        FROM   tracks t
        LEFT JOIN albums al ON al.id = t.album_id
        "#
    )
            .fetch_all(&self.pool)
            .await?;

        let mut tracks = Vec::with_capacity(rows.len());

        for row in rows {
            let artists = self.get_artists_for_track(&row.uuid).await?;

            let album = match (row.album_id, row.album_name) {
                (Some(id), Some(name)) => Some(Album { id, name }),
                _                      => None,
            };

            tracks.push(Track {
                id:               row.uuid,
                title:            row.title,
                duration_seconds: row.duration_seconds,
                thumbnail_small:  row.thumbnail_small,
                thumbnail_large:  row.thumbnail_large,
                bpm:              row.bpm,
                camelot_key:      row.camelot_key,
                file_path:        row.file_path,
                added_at:         row.added_at,
                album,
                artists,
            });
        }

        Ok(tracks)
    }

    /// Resuelve hasta 250 IDs en 2 queries planas (sin N+1).
    /// Devuelve solo los que existen en BD; los ausentes el caller los busca en YT.
    pub async fn get_many_by_ids(&self, ids: &[String]) -> Result<Vec<Track>, RepositoryError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        // Query 1: tracks + álbumes
        let rows = sqlx::query!(
            r#"
            SELECT
                t.uuid,
                t.title,
                t.duration_seconds,
                t.thumbnail_small,
                t.thumbnail_large,
                t.bpm,
                t.camelot_key,
                t.file_path,
                t.added_at,
                al.id   AS "album_id?",
                al.name AS "album_name?"
            FROM   tracks t
            LEFT JOIN albums al ON al.id = t.album_id
            WHERE  t.uuid = ANY($1)
            "#,
            ids
        )
            .fetch_all(&self.pool)
            .await?;

        if rows.is_empty() {
            return Ok(vec![]);
        }

        let found_ids: Vec<String> = rows.iter().map(|r| r.uuid.clone()).collect();

        // Query 2: todos los artistas de todos esos tracks de una vez
        let artist_rows = sqlx::query!(
            r#"
            SELECT ta.track_uuid, a.id, a.name
            FROM   track_artists ta
            JOIN   artists a ON a.id = ta.artist_id
            WHERE  ta.track_uuid = ANY($1)
            "#,
            &found_ids
        )
            .fetch_all(&self.pool)
            .await?;

        // Agrupar artistas por track_uuid
        let mut artists_map: std::collections::HashMap<String, Vec<Artist>> =
            std::collections::HashMap::new();
        for row in artist_rows {
            artists_map
                .entry(row.track_uuid)
                .or_default()
                .push(Artist { id: row.id, name: row.name });
        }

        let tracks = rows
            .into_iter()
            .map(|row| {
                let artists = artists_map.remove(&row.uuid).unwrap_or_default();
                let album = match (row.album_id, row.album_name) {
                    (Some(id), Some(name)) => Some(Album { id, name }),
                    _                      => None,
                };
                Track {
                    id:               row.uuid,
                    title:            row.title,
                    duration_seconds: row.duration_seconds,
                    thumbnail_small:  row.thumbnail_small,
                    thumbnail_large:  row.thumbnail_large,
                    bpm:              row.bpm,
                    camelot_key:      row.camelot_key,
                    file_path:        row.file_path,
                    added_at:         row.added_at,
                    album,
                    artists,
                }
            })
            .collect();

        Ok(tracks)
    }

    pub async fn get_all_ids(&self) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query!("SELECT uuid FROM tracks")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.uuid).collect())
    }

    // ── Escritura ─────────────────────────────────────────────────────────────

    /// Inserta un track completo (upsert).
    /// Orden dentro de la transacción: álbum → artistas → track → track_artists.
    /// El ON CONFLICT en tracks solo actualiza file_path para no pisar metadatos.
    pub async fn insert(&self, track: &Track) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;

        // 1. Álbum
        if let Some(album) = &track.album {
            sqlx::query!(
                r#"
                INSERT INTO albums (id, name)
                VALUES ($1, $2)
                ON CONFLICT (id) DO NOTHING
                "#,
                album.id,
                album.name,
            )
                .execute(&mut *tx)
                .await?;
        }

        // 2. Artistas
        for artist in &track.artists {
            sqlx::query!(
                r#"
                INSERT INTO artists (id, name)
                VALUES ($1, $2)
                ON CONFLICT (id) DO NOTHING
                "#,
                artist.id,
                artist.name,
            )
                .execute(&mut *tx)
                .await?;
        }

        // 3. Track
        sqlx::query!(
            r#"
            INSERT INTO tracks (
                uuid, title, duration_seconds,
                album_id, thumbnail_small, thumbnail_large,
                bpm, camelot_key, file_path
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (uuid) DO UPDATE SET
                file_path = EXCLUDED.file_path
            "#,
            track.id,
            track.title,
            track.duration_seconds,
            track.album.as_ref().map(|a| &a.id),
            track.thumbnail_small,
            track.thumbnail_large,
            track.bpm,
            track.camelot_key,
            track.file_path,
        )
            .execute(&mut *tx)
            .await?;

        // 4. Relaciones track → artistas
        for artist in &track.artists {
            sqlx::query!(
                r#"
                INSERT INTO track_artists (track_uuid, artist_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
                track.id,
                artist.id,
            )
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn update_played(&self, id: &str) -> Result<(), RepositoryError> {
        sqlx::query!(
        r#"
        UPDATE tracks
        SET play_count = COALESCE(play_count, 0) + 1,
            last_played_at = NOW()
        WHERE uuid = $1
        "#,
        id
    )
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Actualiza únicamente el file_path de un track existente.
    pub async fn update_path(&self, id: &str, path: &str) -> Result<(), RepositoryError> {
        sqlx::query!(
            "UPDATE tracks SET file_path = $1 WHERE uuid = $2",
            path,
            id,
        )
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_track(&self, uuid: &str) -> Result<(), RepositoryError> {
        // 1. Validar que existe
        let track = self.get_by_id(uuid).await?
            .ok_or_else(|| RepositoryError::Custom(format!("Track {} no encontrado", uuid)))?;

        // 2. Borrado físico
        if let Some(file_path) = track.file_path {
            match tokio::fs::remove_file(&file_path).await {
                Ok(_) => info!("Archivo físico eliminado: {}", file_path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    info!("El archivo físico ya no existía: {}", file_path);
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }

        // 3. Borrado lógico
        self.delete_db_record(uuid).await?;
        info!("Track {} purgado de la base de datos", uuid);

        Ok(())
    }
    // }

    // ── Helpers internos ──────────────────────────────────────────────────────

    async fn get_artists_for_track(&self, track_id: &str) -> Result<Vec<Artist>, RepositoryError> {
        let rows = sqlx::query!(
            r#"
            SELECT a.id, a.name
            FROM   track_artists ta
            JOIN   artists a ON a.id = ta.artist_id
            WHERE  ta.track_uuid = $1
            "#,
            track_id
        )
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|r| Artist { id: r.id, name: r.name }).collect())
    }

    pub async fn delete_db_record(&self, uuid: &str) -> Result<(), RepositoryError> {
        sqlx::query!(
            "DELETE FROM tracks WHERE uuid = $1",
            uuid
        )
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_analysis_data(&self, id: &str, bpm: Option<i32>, camelot_key: Option<String>) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE tracks
            SET bpm = $1, camelot_key = $2
            WHERE uuid = $3
            "#,
            bpm,
            camelot_key,
            id
        )
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}