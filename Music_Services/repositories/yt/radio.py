from ytmusicapi import YTMusic
from models.track import Track
from repositories.yt._mapper import map_artists, map_album, best_thumbnail


class YTMusicRadioRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_radio_tracks(self, seed_track_id: str, limit: int = 25) -> tuple[Track, ...]:
        raw_playlist = self._client.get_watch_playlist(videoId=seed_track_id, limit=limit)
        raw_tracks = raw_playlist.get('tracks', [])
        return tuple(self._map_radio_track(item) for item in raw_tracks)

    @staticmethod
    def _map_radio_track(item: dict) -> Track:
        artists = map_artists(item.get('artists', []))
        album_ref = map_album(item.get('album'))

        return Track(
            id=item.get('videoId', ''),
            title=item.get('title', ''),
            artists=artists,
            duration_seconds=int(item.get('lengthSec') or item.get('duration_seconds') or 0),
            thumbnail=best_thumbnail(item.get('thumbnails', [])),
            album=album_ref
        )