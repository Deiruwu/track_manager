from typing import Optional, Literal
from ytmusicapi import YTMusic
from models.track import Track
from repositories.yt._mapper import map_track

SearchType = Literal["songs", "videos"]

class YTMusicSearchRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_first(self, query: str, type: SearchType = "songs") -> Optional[Track]:
        results = await self.search(query, type, limit=1)
        return results[0] if results else None

    async def search(self, query: str, type: SearchType = "songs", limit: int = 5) -> tuple[Track, ...]:
        raw = self._client.search(query=query, filter=type)
        return tuple(map_track(item) for item in raw[:limit])