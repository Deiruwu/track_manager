import re

from models.track import Track
from models.artist import ArtistRef
from models.album import AlbumRef
from models.shared import Thumbnail

_SIZE_SUFFIX = re.compile(r'=w\d+-h\d+')


def map_artists(raw: list) -> tuple[ArtistRef, ...]:
    return tuple(
        ArtistRef(id=a.get('id', ''), name=a.get('name', ''))
        for a in raw
    )


def map_album(raw: dict | None) -> AlbumRef | None:
    if not raw:
        return None
    return AlbumRef(id=raw.get('id', ''), name=raw.get('name', ''))


def parse_duration(item: dict) -> int:
    """Segundos del item: `duration_seconds` si viene, si no el texto
    `duration`/`length` ("3:45", "1:02:03") o `lengthSec`. 0 si no hay nada
    (p. ej. las canciones top de get_artist, que no traen duración)."""
    seconds = item.get('duration_seconds') or item.get('lengthSec')
    if seconds:
        try:
            return int(seconds)
        except (TypeError, ValueError):
            pass
    raw = item.get('duration') or item.get('length')
    if isinstance(raw, str):
        try:
            total = 0
            for part in raw.split(':'):
                total = total * 60 + int(part)
            return total
        except ValueError:
            pass
    return 0


def map_track(item: dict) -> Track:
    small, large = best_thumbnails(item.get('thumbnails', []))
    return Track(
        id=item.get('videoId', ''),
        title=item.get('title', ''),
        artists=map_artists(item.get('artists', [])),
        duration_seconds=parse_duration(item),
        thumbnail_small=small,
        thumbnail_large=large,
        album=map_album(item.get('album'))
    )


def _force_size(url: str, size: int) -> Thumbnail | None:
    """Pide directamente el tamaño que hace falta en vez de conformarnos con
    lo que la API haya listado — el CDN de avatares de canal
    (yt3.googleusercontent.com) respeta cualquier '=wN-hN' pedido (verificado
    en vivo). Así no quedamos pegados a un tamaño chico si la API algún día
    lista menos opciones de las que en realidad tiene disponibles."""
    if not url or not _SIZE_SUFFIX.search(url):
        return None
    return Thumbnail(url=_SIZE_SUFFIX.sub(f'=w{size}-h{size}', url, count=1), width=size, height=size)


def best_thumbnails(
    raw: list,
    sizes: tuple[int, int] | None = (120, 544),
) -> tuple[Thumbnail | None, Thumbnail | None]:
    """sizes=None conserva el comportamiento original (el más grande que
    haya en la lista, sin forzar nada) — lo necesita el banner panorámico
    del artista, que no debe recortarse a cuadrado."""
    if not raw:
        return None, None
    mapped = [
        Thumbnail(url=t.get('url', ''), width=t.get('width', 0), height=t.get('height', 0))
        for t in raw
    ]

    large = max(mapped, key=lambda t: t.height)

    if sizes:
        forced_small = _force_size(large.url, sizes[0])
        forced_large = _force_size(large.url, sizes[1])
        if forced_small and forced_large:
            return forced_small, forced_large

    candidates = [t for t in mapped if t.height >= 120]
    small = min(candidates, key=lambda t: t.height) if candidates else large

    return small, large

def map_thumbnails_nested(vid: dict) -> tuple[Thumbnail | None, Thumbnail | None]:
    raw = vid.get('thumbnail', {}).get('thumbnails', [])
    return best_thumbnails(raw)