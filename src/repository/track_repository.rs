use chrono::{DateTime, Utc};
use sqlx::{FromRow, QueryBuilder};
use tracing::info;
use crate::infrastructure::{Db, DbPool};
use crate::model::{Album, Artist, Track};
use crate::repository::errors::RepositoryError;

/// Columnas de `tracks` + álbum unido, tal como las devuelve `TRACK_SELECT`.
#[derive(FromRow)]
struct TrackRow {
    uuid:             String,
    title:            String,
    duration_seconds: i32,
    thumbnail_small:  Option<String>,
    thumbnail_large:  Option<String>,
    bpm:              Option<i32>,
    camelot_key:      Option<String>,
    file_path:        Option<String>,
    added_at:         Option<DateTime<Utc>>,
    album_id:         Option<String>,
    album_name:       Option<String>,
}

/// Artista unido a la pista a la que pertenece.
#[derive(FromRow)]
struct TrackArtistRow {
    track_uuid: String,
    id:         String,
    name:       String,
}

/// SELECT base de tracks con su álbum; cada caller agrega su WHERE.
const TRACK_SELECT: &str = r#"
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
        al.id   AS album_id,
        al.name AS album_name
    FROM   tracks t
    LEFT JOIN albums al ON al.id = t.album_id
"#;

#[derive(Clone)]
pub struct TrackRepository {
    pool: DbPool,
}

impl TrackRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Ignora rutas que se sabe que no son audio (el .lrc que se coló en
    /// file_path por el bug ya corregido en DownloadService::find_file) — se
    /// trata como si la pista no tuviera archivo, así que `download_track`
    /// la vuelve a resolver. No se exige una extensión de audio específica:
    /// la librería puede tener pistas en .opus, .flac, .m4a, etc.
    fn valid_audio_path(path: Option<String>) -> Option<String> {
        path.filter(|p| !p.ends_with(".lrc"))
    }

    /// Arma el `Track` de dominio a partir de la fila y sus artistas.
    fn track_from_row(row: TrackRow, artists: Vec<Artist>) -> Track {
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
            file_path:        Self::valid_audio_path(row.file_path),
            added_at:         row.added_at,
            album,
            artists,
        }
    }

    // ── Lectura ───────────────────────────────────────────────────────────────

    /// Busca un track por su ID, resolviendo álbum y artistas en el mismo viaje.
    /// Devuelve `None` si no existe.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<Track>, RepositoryError> {
        let row = sqlx::query_as::<_, TrackRow>(&format!("{TRACK_SELECT} WHERE t.uuid = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        let row = match row {
            Some(r) => r,
            None    => return Ok(None),
        };

        let artists = self.get_artists_for_track(id).await?;

        Ok(Some(Self::track_from_row(row, artists)))
    }

    pub async fn get_all(&self) -> Result<Vec<Track>, RepositoryError> {
        let rows = sqlx::query_as::<_, TrackRow>(TRACK_SELECT)
            .fetch_all(&self.pool)
            .await?;

        let mut tracks = Vec::with_capacity(rows.len());

        for row in rows {
            let artists = self.get_artists_for_track(&row.uuid).await?;
            tracks.push(Self::track_from_row(row, artists));
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
        let mut query = QueryBuilder::<Db>::new(TRACK_SELECT);
        query.push(" WHERE t.uuid IN (");
        let mut list = query.separated(", ");
        for id in ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");

        let rows = query
            .build_query_as::<TrackRow>()
            .fetch_all(&self.pool)
            .await?;

        if rows.is_empty() {
            return Ok(vec![]);
        }

        // Query 2: todos los artistas de todos esos tracks de una vez
        let mut query = QueryBuilder::<Db>::new(
            r#"
            SELECT ta.track_uuid, a.id, a.name
            FROM   track_artists ta
            JOIN   artists a ON a.id = ta.artist_id
            WHERE  ta.track_uuid IN ("#,
        );
        let mut list = query.separated(", ");
        for row in &rows {
            list.push_bind(row.uuid.clone());
        }
        list.push_unseparated(")");

        let artist_rows = query
            .build_query_as::<TrackArtistRow>()
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
                Self::track_from_row(row, artists)
            })
            .collect();

        Ok(tracks)
    }

    pub async fn get_all_ids(&self) -> Result<Vec<String>, RepositoryError> {
        let ids = sqlx::query_scalar::<_, String>("SELECT uuid FROM tracks")
            .fetch_all(&self.pool)
            .await?;

        Ok(ids)
    }

    // ── Escritura ─────────────────────────────────────────────────────────────

    /// Inserta un track completo (upsert).
    /// Orden dentro de la transacción: álbum → artistas → track → track_artists.
    /// El ON CONFLICT en tracks solo actualiza file_path para no pisar metadatos.
    pub async fn insert(&self, track: &Track) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;

        // 1. Álbum
        if let Some(album) = &track.album {
            sqlx::query(r#"
                INSERT INTO albums (id, name)
                VALUES ($1, $2)
                ON CONFLICT (id) DO NOTHING
                "#)
                .bind(&album.id)
                .bind(&album.name)
                .execute(&mut *tx)
                .await?;
        }

        // 2. Artistas
        for artist in &track.artists {
            sqlx::query(r#"
                INSERT INTO artists (id, name)
                VALUES ($1, $2)
                ON CONFLICT (id) DO NOTHING
                "#)
                .bind(&artist.id)
                .bind(&artist.name)
                .execute(&mut *tx)
                .await?;
        }

        // 3. Track
        sqlx::query(r#"
            INSERT INTO tracks (
                uuid, title, duration_seconds,
                album_id, thumbnail_small, thumbnail_large,
                bpm, camelot_key, file_path
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (uuid) DO UPDATE SET
                file_path        = EXCLUDED.file_path,
                duration_seconds = CASE WHEN EXCLUDED.duration_seconds > 0
                                        THEN EXCLUDED.duration_seconds
                                        ELSE tracks.duration_seconds END
            "#)
            .bind(&track.id)
            .bind(&track.title)
            .bind(&track.duration_seconds)
            .bind(track.album.as_ref().map(|a| &a.id))
            .bind(&track.thumbnail_small)
            .bind(&track.thumbnail_large)
            .bind(&track.bpm)
            .bind(&track.camelot_key)
            .bind(&track.file_path)
            .execute(&mut *tx)
            .await?;

        // 4. Relaciones track → artistas
        for artist in &track.artists {
            sqlx::query(r#"
                INSERT INTO track_artists (track_uuid, artist_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#)
                .bind(&track.id)
                .bind(&artist.id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Actualiza título/álbum/artistas de un track usando datos "de contexto" ya
    /// confiables (item de álbum/artista), sin necesitar que esté descargado.
    /// La duración solo se pisa si la nueva es > 0: las canciones top de un
    /// artista llegan sin duración y no deben borrar una ya conocida.
    /// No toca file_path, bpm ni camelot_key — así no pisa el estado de descarga
    /// ni el análisis ya guardados. Reemplaza por completo las relaciones
    /// track_artists para no dejar artistas fantasma de una resolución previa
    /// envenenada (p.ej. un `yt_gen_` fabricado por falta de id real).
    pub async fn upsert_track_metadata(&self, track: &Track) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;

        if let Some(album) = &track.album {
            sqlx::query(r#"
                INSERT INTO albums (id, name)
                VALUES ($1, $2)
                ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name
                "#)
                .bind(&album.id)
                .bind(&album.name)
                .execute(&mut *tx)
                .await?;
        }

        for artist in &track.artists {
            sqlx::query(r#"
                INSERT INTO artists (id, name)
                VALUES ($1, $2)
                ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name
                "#)
                .bind(&artist.id)
                .bind(&artist.name)
                .execute(&mut *tx)
                .await?;
        }

        sqlx::query(r#"
            INSERT INTO tracks (
                uuid, title, duration_seconds,
                album_id, thumbnail_small, thumbnail_large
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (uuid) DO UPDATE SET
                title            = EXCLUDED.title,
                album_id         = EXCLUDED.album_id,
                thumbnail_small  = EXCLUDED.thumbnail_small,
                thumbnail_large  = EXCLUDED.thumbnail_large,
                duration_seconds = CASE WHEN EXCLUDED.duration_seconds > 0
                                        THEN EXCLUDED.duration_seconds
                                        ELSE tracks.duration_seconds END
            "#)
            .bind(&track.id)
            .bind(&track.title)
            .bind(&track.duration_seconds)
            .bind(track.album.as_ref().map(|a| &a.id))
            .bind(&track.thumbnail_small)
            .bind(&track.thumbnail_large)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM track_artists WHERE track_uuid = $1")
            .bind(&track.id)
            .execute(&mut *tx)
            .await?;

        for artist in &track.artists {
            sqlx::query(r#"
                INSERT INTO track_artists (track_uuid, artist_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#)
                .bind(&track.id)
                .bind(&artist.id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn update_played(&self, id: &str) -> Result<(), RepositoryError> {
        sqlx::query(r#"
        UPDATE tracks
        SET play_count = COALESCE(play_count, 0) + 1,
            last_played_at = $1
        WHERE uuid = $2
        "#)
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Actualiza únicamente el file_path de un track existente.
    pub async fn update_path(&self, id: &str, path: &str) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE tracks SET file_path = $1 WHERE uuid = $2")
            .bind(path)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_track(&self, uuid: &str) -> Result<(), RepositoryError> {
        // 1. Validar que existe
        let track = self.get_by_id(uuid).await?
            .ok_or_else(|| RepositoryError::Custom(format!("Track {} no encontrado", uuid)))?;

        // 2. Borrado físico (audio + letra asociada)
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

            let lyrics_path = std::path::Path::new(&file_path).with_extension("lrc");
            match tokio::fs::remove_file(&lyrics_path).await {
                Ok(_) => info!("Letra eliminada: {}", lyrics_path.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
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
        let rows = sqlx::query_as::<_, (String, String)>(r#"
            SELECT a.id, a.name
            FROM   track_artists ta
            JOIN   artists a ON a.id = ta.artist_id
            WHERE  ta.track_uuid = $1
            "#)
            .bind(track_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|(id, name)| Artist { id, name }).collect())
    }

    pub async fn delete_db_record(&self, uuid: &str) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM tracks WHERE uuid = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_analysis_data(&self, id: &str, bpm: Option<i32>, camelot_key: Option<String>) -> Result<(), RepositoryError> {
        sqlx::query(r#"
            UPDATE tracks
            SET bpm = $1, camelot_key = $2
            WHERE uuid = $3
            "#)
            .bind(bpm)
            .bind(camelot_key)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}