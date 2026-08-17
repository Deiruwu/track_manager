from dataclasses import dataclass
from typing import Optional

from models.shared import Thumbnail

@dataclass(frozen=True, slots=True)
class ArtistRef:
    """
    Referencia inmutable a un artista.
    Usado como composición dentro de otras entidades (ej. Track, Album)
    para evitar overfetching del perfil completo del artista.
    """
    id: str
    name: str

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name
        }


@dataclass(frozen=True, slots=True)
class ArtistDetail:
    id: str
    name: str
    banner: Optional[Thumbnail]
    songs: tuple
    albums: tuple

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "banner": self.banner.url if self.banner else None,
            "songs": [song.to_dict() for song in self.songs],
            "albums": [album.to_dict() for album in self.albums],
        }