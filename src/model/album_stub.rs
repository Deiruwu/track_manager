use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumStub {
    pub id:              String,
    pub name:            String,
    pub thumbnail_small: Option<String>,
    pub thumbnail_large: Option<String>,
    #[serde(rename = "type")]
    pub kind:            Option<String>,
    pub year:            Option<String>,
}
