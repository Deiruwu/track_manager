from typing import Optional
from ytmusicapi import YTMusic
from models.shared import Thumbnail
from models.track import Track
from models.album import AlbumStub
from models.artist import ArtistDetail
from repositories.yt._mapper import map_track, best_thumbnails


class YTMusicArtistRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def get_artist_overview(self, artist_id: str, song_limit: int = 5) -> ArtistDetail:
        raw = self._client.get_artist(channelId=artist_id)
        name = raw.get('name', '')

        # El thumbnail de get_artist es un banner ancho, no una foto de perfil.
        _, banner = best_thumbnails(raw.get('thumbnails', []))
        avatar_small, avatar_large = self._get_avatar(name, artist_id)

        song_results = raw.get('songs', {}).get('results', [])[:song_limit]
        songs = tuple(map_track(item) for item in song_results)

        albums = self._collect_discography(raw.get('albums'), default_type='Album')
        singles = self._collect_discography(raw.get('singles'), default_type='Single')
        discography = tuple(sorted(albums + singles, key=self._year_sort_key, reverse=True))

        return ArtistDetail(
            id=artist_id,
            name=name,
            banner=banner,
            avatar_small=avatar_small,
            avatar_large=avatar_large,
            songs=songs,
            albums=discography,
        )

    def _get_avatar(self, name: str, artist_id: str) -> tuple[Optional[Thumbnail], Optional[Thumbnail]]:
        """
        get_artist() no expone la foto de perfil real, sólo el banner.
        Buscar por channelId crudo no es confiable (falla para algunos artistas);
        buscar por nombre y quedarse sólo con el browseId que coincide sí lo es.
        """
        if not name:
            return None, None

        try:
            results = self._client.search(query=name, filter="artists")
        except Exception:
            return None, None

        match = next((r for r in results if r.get('browseId') == artist_id), None)
        if not match:
            return None, None

        # best_thumbnails() exige >=120px para la variante "small" (pensado
        # para tracks/álbumes); el avatar sólo trae 60x60 y 120x120, así que
        # con ese umbral ambas variantes colapsarían en la misma imagen.
        thumbnails = match.get('thumbnails', [])
        if not thumbnails:
            return None, None

        mapped = [
            Thumbnail(url=t.get('url', ''), width=t.get('width', 0), height=t.get('height', 0))
            for t in thumbnails
        ]
        small = min(mapped, key=lambda t: t.height)
        large = max(mapped, key=lambda t: t.height)
        return small, large

    def _collect_discography(self, block: dict | None, default_type: str) -> tuple[AlbumStub, ...]:
        if not block:
            return ()

        browse_id = block.get('browseId')
        params = block.get('params')
        if browse_id and params:
            # Catálogo completo, ya ordenado por fecha desde YT Music.
            items = self._client.get_artist_albums(channelId=browse_id, params=params, order='Recency')
        else:
            # Artista con pocos lanzamientos: la preview de get_artist ya trae todo.
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
