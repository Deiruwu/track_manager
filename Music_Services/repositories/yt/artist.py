import asyncio
from dataclasses import replace
from ytmusicapi import YTMusic
from models.track import Track
from models.album import AlbumStub
from models.artist import ArtistDetail, ArtistProfile, ArtistRef
from repositories.yt._mapper import map_track, best_thumbnails, parse_duration


def _parse_views(raw: str | None) -> int | None:
    """'430,148,803 views' -> 430148803. Entero crudo, sin formatear ni
    abreviar, para que cada consumidor (Rust incluido) decida cómo mostrarlo."""
    if not raw:
        return None
    digits = ''.join(ch for ch in raw if ch.isdigit())
    return int(digits) if digits else None


class YTMusicArtistRepository:
    def __init__(self, client: YTMusic):
        self._client = client

    async def _fetch_artist(self, artist_id: str) -> dict:
        try:
            return await asyncio.to_thread(self._client.get_artist, channelId=artist_id)
        except KeyError as e:
            # ytmusicapi solo sabe parsear el header "inmersivo" que usan los
            # artistas musicales canónicos de YT Music. Un channelId de un
            # canal normal de YouTube (no registrado como Music Artist) trae
            # otro tipo de header (musicVisualHeaderRenderer) y la librería
            # revienta con KeyError en vez de devolver algo manejable.
            raise ValueError(
                f"El canal '{artist_id}' no tiene un perfil de artista musical "
                f"en YouTube Music (falta {e} en la respuesta)"
            ) from e

    async def get_artist_profile(self, artist_id: str) -> ArtistProfile:
        """Versión dummy: una sola llamada a get_artist, sin canciones ni discografía."""
        raw = await self._fetch_artist(artist_id)
        # Mismos tamaños que usa Album (models/album.py) para thumbnail_small/large.
        thumbnail_small, thumbnail_large = best_thumbnails(raw.get('thumbnails', []))
        return ArtistProfile(
            id=artist_id,
            name=raw.get('name', ''),
            thumbnail_small=thumbnail_small,
            thumbnail_large=thumbnail_large,
        )

    async def get_artist_overview(self, artist_id: str, song_limit: int = 5) -> ArtistDetail:
        raw = await self._fetch_artist(artist_id)
        name = raw.get('name', '')
        views = _parse_views(raw.get('views'))  # entero crudo, p. ej. 430148803

        # sizes=None: el banner es panorámico, no debe forzarse a cuadrado.
        _, banner = best_thumbnails(raw.get('thumbnails', []), sizes=None)

        song_results = raw.get('songs', {}).get('results', [])[:song_limit]
        self_ref = ArtistRef(id=artist_id, name=name)
        songs = tuple(
            song if any(a.id for a in song.artists) else replace(song, artists=(self_ref,))
            for song in (map_track(item) for item in song_results)
        )

        albums, singles, durations = await asyncio.gather(
            asyncio.to_thread(self._collect_discography, raw.get('albums'), 'Album'),
            asyncio.to_thread(self._collect_discography, raw.get('singles'), 'Single'),
            asyncio.to_thread(self._song_durations, raw.get('songs'), songs, song_limit),
        )
        # get_artist no trae la duración de las canciones top: sale de la playlist "Canciones" del artista.
        songs = tuple(
            replace(song, duration_seconds=durations.get(song.id, 0)) if not song.duration_seconds else song
            for song in songs
        )
        # Sin ordenar: Rust ordena por año al armar ArtistResult (track_manager.rs).
        discography = albums + singles

        # "A los fans también les gusta": ya viene en la misma respuesta de get_artist.
        related = tuple(
            self._map_related_artist(item)
            for item in (raw.get('related') or {}).get('results', [])
            if item.get('browseId')
        )

        return ArtistDetail(
            id=artist_id,
            name=name,
            banner=banner,
            views=views,
            songs=songs,
            albums=discography,
            related=related,
        )

    def _song_durations(self, block: dict | None, songs: tuple[Track, ...], song_limit: int) -> dict[str, int]:
        """Duración por videoId de las canciones top, leída de la playlist del
        bloque "Canciones" (que sí la trae). Vacío si no hace falta o falla:
        quedan en 0 y Rust usa la duración guardada en la BD, si la hay."""
        playlist_id = (block or {}).get('browseId')
        if not playlist_id or all(song.duration_seconds for song in songs):
            return {}
        try:
            # Margen sobre song_limit: la playlist puede no venir en el mismo orden que el bloque.
            playlist = self._client.get_playlist(playlist_id, limit=max(song_limit * 4, 25))
        except Exception as e:
            print(f"[get_artist_overview] Sin duraciones de {playlist_id}: {e}")
            return {}
        return {
            item['videoId']: parse_duration(item)
            for item in playlist.get('tracks') or []
            if item.get('videoId')
        }

    def _collect_discography(self, block: dict | None, default_type: str) -> tuple[AlbumStub, ...]:
        if not block:
            return ()

        browse_id = block.get('browseId')
        params = block.get('params')
        if browse_id and params:
            # Sin order='Recency': ese parámetro le cuesta a ytmusicapi una
            # petición HTTP extra (pide el menú de orden, luego reaplica) y
            # el resultado se reordena de todos modos del lado de Rust.
            items = self._client.get_artist_albums(channelId=browse_id, params=params)
        else:
            items = block.get('results', [])

        return tuple(self._map_album_stub(item, default_type) for item in items)

    @staticmethod
    def _map_related_artist(item: dict) -> ArtistProfile:
        small, large = best_thumbnails(item.get('thumbnails') or [])
        return ArtistProfile(
            id=item['browseId'],
            name=item.get('title', ''),
            thumbnail_small=small,
            thumbnail_large=large,
        )

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
