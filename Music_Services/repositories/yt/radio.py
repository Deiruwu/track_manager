import asyncio
from ytmusicapi import YTMusic
from models.track import Track
from repositories.yt._mapper import map_artists, map_album, best_thumbnails, parse_duration


class YTMusicRadioRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_radio_tracks(self, seed_track_id: str, limit: int = 25) -> tuple[Track, ...]:
        raw_playlist = await asyncio.to_thread(self._client.get_watch_playlist, videoId=seed_track_id, limit=limit)
        raw_tracks = raw_playlist.get('tracks', [])
        return tuple(self._map_radio_track(item) for item in raw_tracks)

    @staticmethod
    def _map_radio_track(item: dict) -> Track:
        raw_thumbs = item.get('thumbnails') or item.get('thumbnail') or []
        small, large = best_thumbnails(raw_thumbs)
        return Track(
            id=item.get('videoId', ''),
            title=item.get('title', ''),
            artists=map_artists(item.get('artists', [])),
            duration_seconds=parse_duration(item),
            thumbnail_small=small,
            thumbnail_large=large,
            album=map_album(item.get('album'))
        )