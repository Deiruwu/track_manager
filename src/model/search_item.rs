use serde::{Deserialize, Serialize};
use crate::model::{Artist, ArtistProfileResult, Track};

/// Álbum tal como aparece en una búsqueda: stub + artistas acreditados.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumSearchResult {
    pub id:              String,
    pub name:            String,
    pub thumbnail_small: Option<String>,
    pub thumbnail_large: Option<String>,
    #[serde(rename = "type")]
    pub kind:            Option<String>,
    pub year:            Option<String>,
    #[serde(default)]
    pub artists:         Vec<Artist>,
}

/// Resultado heterogéneo de la acción "search_items", tagueado por "kind".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SearchItem {
    Track(Track),
    Album(AlbumSearchResult),
    Artist(ArtistProfileResult),
}
