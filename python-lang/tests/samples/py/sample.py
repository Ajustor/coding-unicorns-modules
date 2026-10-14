"""Module docstring."""
from dataclasses import dataclass, field
import re


@dataclass
class Item:
    name: str
    tags: list[str] = field(default_factory=list)

    def matches(self, pattern: str) -> bool:
        """Whether the name matches."""
        return re.search(pattern, self.name) is not None


async def main(*args, **kwargs) -> None:
    items = [Item("a"), Item(name="b", tags=["x"])]
    found = {i.name: len(i.tags) for i in items if i.matches(r"^\w$")}
    print(f"found {found!r} in {len(items)} items", 0x1F, 3.14, None, True)
    # comment
    with open("f.txt") as fh, lambda_ctx() as _:
        pass
    raise ValueError("bad") from None
