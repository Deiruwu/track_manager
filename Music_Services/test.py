import asyncio
import time
from ytmusicapi import YTMusic

from repositories.yt.search import YTMusicSearchRepository
from repositories.yt.track import YTMusicTrackRepository
from repositories.yt.album import YTMusicAlbumRepository
from repositories.yt.artist import YTMusicArtistRepository


async def test():
    yt = YTMusic()
    search_repo = YTMusicSearchRepository(yt)
    track_repo = YTMusicTrackRepository(yt)
    album_repo = YTMusicAlbumRepository(yt)
    artist_repo = YTMusicArtistRepository(yt)

    query = "DMGIMP1fMwg"
    query_id = "MPREb_kAUqYH0O1kL"
    artist_ids = [
        "UCSMsV0YVTmlh_XpkCFW3Xmw",
        "UC-ShhdQ4SYoJvh2JH6qmPvA",
    ]

    start = time.perf_counter()
    album = await album_repo.get_album_detail(query_id)
    end = time.perf_counter()
    print(f"[ALBUM] {album.name} ({album.album_type}, {album.year}) — {len(album.tracks)} tracks")
    for track in album.tracks:
        print(track)
    print(f"[TIME] get_album_detail: {end - start:.4f}s\n")

    for artist_id in artist_ids:
        start = time.perf_counter()
        artist = await artist_repo.get_artist_overview(artist_id, song_limit=5)
        end = time.perf_counter()
        print(f"[ARTIST] {artist.name} — {len(artist.songs)} top songs, {len(artist.albums)} discography entries")
        print(f"  banner: {artist.banner.url if artist.banner else None}")
        for song in artist.songs:
            print(song)
        for album_stub in artist.albums:
            print(f"  {album_stub.year} [{album_stub.album_type}] {album_stub.name} ({album_stub.id})")
        print(f"[TIME] get_artist_overview: {end - start:.4f}s\n")



if __name__ == "__main__":
    asyncio.run(test())