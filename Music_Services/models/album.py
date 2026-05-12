from dataclasses import dataclass

@dataclass(frozen=True, slots=True)
class AlbumRef:
    """Referencia inmutable al álbum de origen de un track."""
    id: str
    name: str

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name
        }