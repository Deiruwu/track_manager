from ytmusicapi import YTMusic
from models.track import Track
from models.artist import ArtistRef
from models.album import AlbumRef
from repositories.yt._mapper import best_thumbnails


class YTMusicAlbumRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_album_tracks(self, album_id: str) -> tuple[Track, ...]:
        raw = self._client.get_album(browseId=album_id)

        # Contexto del álbum — lo inyectamos en cada track
        album_ref = AlbumRef(
            id=album_id,
            name=raw.get('title', '')
        )

        # Thumbnail del álbum — los tracks no tienen la suya propia
        album_thumbnails = raw.get('thumbnails', [])

        raw_tracks = raw.get('tracks', [])
        return tuple(
            self._map_album_track(item, album_ref, album_thumbnails)
            for item in raw_tracks
        )

    @staticmethod
    def _map_album_track(item: dict, album_ref: AlbumRef, album_thumbnails: list) -> Track:
        artists = tuple(
            ArtistRef(id=a.get('id', ''), name=a.get('name', ''))
            for a in item.get('artists', [])
        )
        track_thumbnails = item.get('thumbnails') or album_thumbnails
        small, large = best_thumbnails(track_thumbnails)
        return Track(
            id=item.get('videoId', ''),
            title=item.get('title', ''),
            artists=artists,
            duration_seconds=item.get('duration_seconds') or 0,
            thumbnail_small=small,
            thumbnail_large=large,
            album=album_ref
        )