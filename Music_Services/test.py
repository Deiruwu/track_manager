import asyncio
import time
from ytmusicapi import YTMusic

from repositories.yt.search import YTMusicSearchRepository
from repositories.yt.track import YTMusicTrackRepository
from repositories.yt.album import YTMusicAlbumRepository


async def test():
    yt = YTMusic()
    search_repo = YTMusicSearchRepository(yt)
    track_repo = YTMusicTrackRepository(yt)
    album_repo = YTMusicAlbumRepository(yt)

    query = "Marsupials SIEMPRE"
    query_id = "MPREb_czhY67Ip8ub"


    # ── 1. Search songs ──
    start = time.perf_counter()
    first_song = await album_repo.get_album_tracks(query_id)
    end = time.perf_counter()
    for track in first_song:
        print(track)


    print(f"[TIME] search songs: {end - start:.4f}s\n")



if __name__ == "__main__":
    asyncio.run(test())