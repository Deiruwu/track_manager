import json
import time

from ytmusicapi import YTMusic

yt = YTMusic()

query = "MPREb_cHM5wZeVqf7"
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
results = yt.get_album(query)
end = time.perf_counter()
print(f"[TIME] search songs: {end - start:.4f}s\n")


print(json.dumps(results, indent=4, ensure_ascii=False))