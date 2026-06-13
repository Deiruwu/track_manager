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
    small, large = best_thumbnails(item.get('thumbnails', []))
    return Track(
        id=item.get('videoId', ''),
        title=item.get('title', ''),
        artists=map_artists(item.get('artists', [])),
        duration_seconds=item.get('duration_seconds') or 0,
        thumbnail_small=small,
        thumbnail_large=large,
        album=map_album(item.get('album'))
    )


def best_thumbnails(raw: list) -> tuple[Thumbnail | None, Thumbnail | None]:
    if not raw:
        return None, None
    mapped = [
        Thumbnail(url=t.get('url', ''), width=t.get('width', 0), height=t.get('height', 0))
        for t in raw
    ]

    large = max(mapped, key=lambda t: t.height)

    candidates = [t for t in mapped if t.height >= 120]
    small = min(candidates, key=lambda t: t.height) if candidates else large

    return small, large

def map_thumbnails_nested(vid: dict) -> tuple[Thumbnail | None, Thumbnail | None]:
    raw = vid.get('thumbnail', {}).get('thumbnails', [])
    return best_thumbnails(raw)