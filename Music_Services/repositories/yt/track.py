from typing import Optional
from ytmusicapi import YTMusic
from models.track import Track
from models.artist import ArtistRef
from repositories.yt._mapper import map_artists, map_album, map_thumbnails_nested


class YTMusicTrackRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_track(self, track_id: str) -> Optional[Track]:
        try:
            raw = self._client.get_song(videoId=track_id)
            vid = raw.get('videoDetails') or raw.get('details')
            if not vid:
                return None

            title = vid.get('title', 'Desconocido')
            author = vid.get('author', 'Desconocido')
            artists, album_ref = await self._enrich(track_id, title, author)

            thumbnail_small, thumbnail_large = map_thumbnails_nested(vid)
            return Track(
                id=track_id,
                title=title,
                artists=artists,
                duration_seconds=int(vid.get('lengthSeconds', 0)),
                thumbnail_small=thumbnail_small,
                thumbnail_large=thumbnail_large,
                album=album_ref
            )
        except Exception as e:
            print(f"[get_track] Error {track_id}: {e}")
            return None

    async def _enrich(self, track_id: str, title: str, author: str):
        try:
            song_results = self._client.search(query=f"{title} {author}", filter="songs")
            match = next((r for r in song_results if r.get('videoId') == track_id), None)

            if not match:
                all_results = self._client.search(query=f"{title} {author}")
                match = next((r for r in all_results if r.get('videoId') == track_id), None)

            if not match:
                return (ArtistRef(id='', name=author),), None

            artists = map_artists(match.get('artists', []))
            album_ref = map_album(match.get('album'))

            return artists, album_ref
        except Exception:
            return (ArtistRef(id='', name=author),), None