from typing import Protocol
from models.track import Track

class SearchRepository(Protocol):
    """
    Contrato estricto para las búsquedas.
    Cualquier implementación debe devolver tuplas inmutables del dominio puro.
    """
    async def search_tracks(self, query: str, limit: int = 5) -> tuple[Track, ...]:
        ...