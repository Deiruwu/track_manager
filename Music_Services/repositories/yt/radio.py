from ytmusicapi import YTMusic
from models.track import Track
from repositories.yt._mapper import map_artists, map_album, best_thumbnail


def _parse_length(item: dict) -> int:
    raw = item.get('length')
    if raw:
        try:
            parts = raw.split(':')
            if len(parts) == 2:
                return int(parts[0]) * 60 + int(parts[1])
            if len(parts) == 3:
                return int(parts[0]) * 3600 + int(parts[1]) * 60 + int(parts[2])
        except (ValueError, AttributeError):
            pass
    return int(item.get('lengthSec') or item.get('duration_seconds') or 0)


class YTMusicRadioRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_radio_tracks(self, seed_track_id: str, limit: int = 25) -> tuple[Track, ...]:
        raw_playlist = self._client.get_watch_playlist(videoId=seed_track_id, limit=limit)
        raw_tracks = raw_playlist.get('tracks', [])
        return tuple(self._map_radio_track(item) for item in raw_tracks)

    @staticmethod
    def _map_radio_track(item: dict) -> Track:
        raw_thumbs = item.get('thumbnails') or item.get('thumbnail') or []

        return Track(
            id=item.get('videoId', ''),
            title=item.get('title', ''),
            artists=map_artists(item.get('artists', [])),
            duration_seconds=_parse_length(item),
            thumbnail=best_thumbnail(raw_thumbs),
            album=map_album(item.get('album'))
        )