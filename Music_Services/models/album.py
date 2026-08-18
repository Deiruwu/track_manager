from dataclasses import dataclass
from typing import Optional

from models.shared import Thumbnail

@dataclass(frozen=True, slots=True)
class AlbumRef:
    """Referencia inmutable al álbum de origen de un track."""
    id: str
    name: str

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name
        }


@dataclass(frozen=True, slots=True)
class AlbumDetail:
    id: str
    name: str
    thumbnail_small: Optional[Thumbnail]
    thumbnail_large: Optional[Thumbnail]
    album_type: Optional[str]
    year: Optional[str]
    tracks: tuple
    artists: tuple = ()

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "thumbnail_small": self.thumbnail_small.url if self.thumbnail_small else None,
            "thumbnail_large": self.thumbnail_large.url if self.thumbnail_large else None,
            "type": self.album_type,
            "year": self.year,
            "artists": [artist.to_dict() for artist in self.artists],
            "tracks": [track.to_dict() for track in self.tracks],
        }


@dataclass(frozen=True, slots=True)
class AlbumStub:
    id: str
    name: str
    thumbnail_small: Optional[Thumbnail]
    thumbnail_large: Optional[Thumbnail]
    album_type: Optional[str]
    year: Optional[str]

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "thumbnail_small": self.thumbnail_small.url if self.thumbnail_small else None,
            "thumbnail_large": self.thumbnail_large.url if self.thumbnail_large else None,
            "type": self.album_type,
            "year": self.year,
        }