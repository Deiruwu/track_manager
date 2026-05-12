from models.track import Track
from models.artist import ArtistRef
from models.album import AlbumRef
from models.shared import Thumbnail


def map_artists(raw: list) -> tuple[ArtistRef, ...]:
    return tuple(
        ArtistRef(id=a.get('id', ''), name=a.get('name', ''))
        for a in raw
    )


def map_album(raw: dict | None) -> AlbumRef | None:
    if not raw:
        return None
    return AlbumRef(id=raw.get('id', ''), name=raw.get('name', ''))


def map_track(item: dict) -> Track:
    return Track(
        id=item.get('videoId', ''),
        title=item.get('title', ''),
        artists=map_artists(item.get('artists', [])),
        duration_seconds=item.get('duration_seconds') or 0,
        thumbnail=best_thumbnail(item.get('thumbnails', [])),
        album=map_album(item.get('album'))
    )


def best_thumbnail(raw: list) -> Thumbnail | None:
    if not raw:
        return None
    mapped = [
        Thumbnail(url=t.get('url', ''), width=t.get('width', 0), height=t.get('height', 0))
        for t in raw
    ]
    exact = next((t for t in mapped if t.width == 120 and t.height == 120), None)
    if exact:
        return exact
    target = 120 * 120
    return min(mapped, key=lambda t: abs((t.width * t.height) - target))


def map_thumbnails_nested(vid: dict) -> Thumbnail | None:
    raw = vid.get('thumbnail', {}).get('thumbnails', [])
    return best_thumbnail(raw)