import asyncio
from dataclasses import replace
from ytmusicapi import YTMusic
from models.track import Track
from models.album import AlbumStub
from models.artist import ArtistDetail, ArtistRef
from repositories.yt._mapper import map_track, best_thumbnails


class YTMusicArtistRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_artist_overview(self, artist_id: str, song_limit: int = 5) -> ArtistDetail:
        raw = await asyncio.to_thread(self._client.get_artist, channelId=artist_id)
        name = raw.get('name', '')

        _, banner = best_thumbnails(raw.get('thumbnails', []))

        song_results = raw.get('songs', {}).get('results', [])[:song_limit]
        self_ref = ArtistRef(id=artist_id, name=name)
        songs = tuple(
            song if any(a.id for a in song.artists) else replace(song, artists=(self_ref,))
            for song in (map_track(item) for item in song_results)
        )

        albums, singles = await asyncio.gather(
            asyncio.to_thread(self._collect_discography, raw.get('albums'), 'Album'),
            asyncio.to_thread(self._collect_discography, raw.get('singles'), 'Single'),
        )
        discography = tuple(sorted(albums + singles, key=self._year_sort_key, reverse=True))

        return ArtistDetail(
            id=artist_id,
            name=name,
            banner=banner,
            songs=songs,
            albums=discography,
        )

    def _collect_discography(self, block: dict | None, default_type: str) -> tuple[AlbumStub, ...]:
        if not block:
            return ()

        browse_id = block.get('browseId')
        params = block.get('params')
        if browse_id and params:
            items = self._client.get_artist_albums(channelId=browse_id, params=params, order='Recency')
        else:
            items = block.get('results', [])

        return tuple(self._map_album_stub(item, default_type) for item in items)

    @staticmethod
    def _map_album_stub(item: dict, default_type: str) -> AlbumStub:
        small, large = best_thumbnails(item.get('thumbnails', []))
        return AlbumStub(
            id=item.get('browseId', ''),
            name=item.get('title', ''),
            thumbnail_small=small,
            thumbnail_large=large,
            album_type=item.get('type') or default_type,
            year=item.get('year'),
        )

    @staticmethod
    def _year_sort_key(album: AlbumStub) -> int:
        return int(album.year) if album.year and album.year.isdigit() else 0
