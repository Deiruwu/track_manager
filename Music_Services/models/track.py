from dataclasses import dataclass
from typing import Optional

from models.shared import Thumbnail
from models.artist import ArtistRef
from models.album import AlbumRef

@dataclass(frozen=True, slots=True)
class Track:
    id: str
    title: str
    artists: tuple[ArtistRef, ...]
    duration_seconds: int
    thumbnail: Optional[Thumbnail]
    album: Optional[AlbumRef] = None

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "title": self.title,
            "artists": [artist.to_dict() for artist in self.artists],
            "duration_seconds": self.duration_seconds,
            "thumbnail": self.thumbnail.to_dict() if self.thumbnail else None,
            "album": self.album.to_dict() if self.album else None,
        }