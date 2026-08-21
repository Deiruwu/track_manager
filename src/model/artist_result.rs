use serde::{Deserialize, Serialize};
use crate::model::{AlbumStub, Track, TrackResult};

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistPayload {
    pub id:     String,
    pub name:   String,
    pub banner: Option<String>,
    /// Vistas totales del canal, entero crudo sin formatear (no confundir
    /// con reproducciones de una canción puntual). i64 porque un canal
    /// grande puede superar el rango de i32 (ej. 3,693,390,359 vistas).
    pub views:  Option<i64>,
    pub songs:  Vec<Track>,
    pub albums: Vec<AlbumStub>,
}

#[derive(Serialize)]
pub struct ArtistResult {
    pub id:     String,
    pub name:   String,
    pub banner: Option<String>,
    pub views:  Option<i64>,
    pub songs:  Vec<TrackResult>,
    pub albums: Vec<AlbumStub>,
}

/// Versión "dummy" del artista: solo id, nombre y foto de perfil — sin banner,
/// canciones ni discografía. Pass-through directo del payload de Python.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArtistProfileResult {
    pub id:    String,
    pub name:  String,
    pub photo: Option<String>,
}
