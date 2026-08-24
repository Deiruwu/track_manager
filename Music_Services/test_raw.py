import json
import time

from ytmusicapi import YTMusic

yt = YTMusic()

query = "UCSMsV0YVTmlh_XpkCFW3Xmw"
query_browser = "MPREb_YU2ompWY08Z"
query_playlist = "OLAK5uy_nWs6TJFLL9wIhnSN9GChSvHd-2w84q7UU"

'''
artists: [
    {
        "name": "Esteman",
        "id": "UChH3W3boDl_9s_0T5aOCv6w"
    },
    {
        "name": "Daniela Spalla",
        "id": "UCLSBwPjNc2kH-3cBn2AbHcA"
    }
],
'''

start = time.perf_counter()
results = yt.get_artist("UCjRlhZ0KDtPQa6yHMoXM3hA")
end = time.perf_counter()
print(f"[TIME] search songs: {end - start:.4f}s\n")


print(json.dumps(results, indent=4, ensure_ascii=False))