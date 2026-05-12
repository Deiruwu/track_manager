from dataclasses import dataclass

@dataclass(frozen=True, slots=True)
class ArtistRef:
    """
    Referencia inmutable a un artista.
    Usado como composición dentro de otras entidades (ej. Track, Album)
    para evitar overfetching del perfil completo del artista.
    """
    id: str
    name: str

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name
        }