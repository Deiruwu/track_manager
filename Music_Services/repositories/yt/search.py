import asyncio
from typing import Optional, Literal
from ytmusicapi import YTMusic
from models.track import Track
from models.album import AlbumSearchResult
from models.artist import ArtistProfile
from repositories.yt._mapper import map_track, map_artists, best_thumbnails

SearchType = Literal["songs", "videos"]
SearchItemType = Literal["songs", "videos", "albums", "artists", "all"]

SEARCH_ITEM_TYPES = ("songs", "videos", "albums", "artists", "all")


class YTMusicSearchRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_first(self, query: str, type: SearchType = "songs") -> Optional[Track]:
        results = await self.search(query, type, limit=1)
        return results[0] if results else None

    async def search(self, query: str, type: SearchType = "songs", limit: int = 5) -> tuple[Track, ...]:
        raw = self._client.search(query=query, filter=type)
        return tuple(map_track(item) for item in raw[:limit])

    async def search_items(self, query: str, type: SearchItemType = "all", limit: int = 5) -> list[dict]:
        """Búsqueda heterogénea: cada item lleva "kind" (track | album | artist).
        type="all" no filtra; descarta lo que no es navegable desde el cliente."""
        filter = None if type == "all" else type
        raw = await asyncio.to_thread(self._client.search, query=query, filter=filter)

        items = []
        for item in raw:
            mapped = map_search_item(item)
            if mapped is not None:
                items.append(mapped)
            if len(items) >= limit:
                break
        return items


def map_search_item(item: dict) -> dict | None:
    """Mapea un resultado crudo de ytmusicapi.search a un dict con "kind"."""
    result_type = item.get('resultType')

    if result_type in ('song', 'video') and item.get('videoId'):
        return {"kind": "track", **map_track(item).to_dict()}

    if result_type == 'album' and item.get('browseId'):
        small, large = best_thumbnails(item.get('thumbnails', []))
        album = AlbumSearchResult(
            id=item['browseId'],
            name=item.get('title', ''),
            thumbnail_small=small,
            thumbnail_large=large,
            album_type=item.get('type'),
            year=item.get('year'),
            artists=tuple(a for a in map_artists(item.get('artists') or []) if a.id),
        )
        return {"kind": "album", **album.to_dict()}

    if result_type == 'artist':
        artists = item.get('artists') or []
        artist_id = item.get('browseId') or (artists[0].get('id') if artists else None)
        name = item.get('artist') or (artists[0].get('name') if artists else None) or item.get('title')
        if not artist_id or not name:
            return None
        small, large = best_thumbnails(item.get('thumbnails', []))
        profile = ArtistProfile(id=artist_id, name=name, thumbnail_small=small, thumbnail_large=large)
        return {"kind": "artist", **profile.to_dict()}

    return None
