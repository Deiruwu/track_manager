from dataclasses import dataclass

@dataclass(frozen=True, slots=True)
class Thumbnail:
    url: str
    width: int
    height: int

    def to_dict(self) -> dict:
        return {
            "url": self.url,
            "width": self.width,
            "height": self.height
        }