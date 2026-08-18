use serde::{Deserialize, Serialize, Serializer};
use serde::ser::SerializeStruct;
use crate::utils::hash::FALLBACK_ID_PREFIX;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Artist {
    pub id:   String,
    pub name: String,
}

impl Artist {
    /// IDs sintéticos (`yt_gen_<hash>`) fabricados por `PythonClient::sanitize`
    /// cuando el artista no tiene un id real. Se guardan tal cual en BD (evita
    /// colisiones entre artistas distintos), pero se ocultan como `null` al
    /// serializar hacia la API.
    pub fn is_fallback_id(&self) -> bool {
        self.id.starts_with(FALLBACK_ID_PREFIX)
    }
}

impl Serialize for Artist {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Artist", 2)?;
        if self.is_fallback_id() {
            state.serialize_field("id", &Option::<&str>::None)?;
        } else {
            state.serialize_field("id", &self.id)?;
        }
        state.serialize_field("name", &self.name)?;
        state.end()
    }
}

