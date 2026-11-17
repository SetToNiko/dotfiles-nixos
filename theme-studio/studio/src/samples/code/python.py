from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

FONT_DIR = Path.home() / ".local/share/fonts"
SAMPLE = "0O 1lI {} [] () => != <= ->"


@dataclass
class Font:
    family: str
    style: str = "Regular"
    monospace: bool = False
    weights: list[int] = field(default_factory=list)

    def label(self) -> str:
        kind = "mono" if self.monospace else "propo"
        return f"{self.family} {self.style} ({kind}, {len(self.weights)} weights)"


class FontListError(Exception):
    pass


def installed_fonts() -> list[Font]:
    # fc-list prints one face per line: family:style:spacing
    try:
        out = subprocess.run(
            ["fc-list", "--format", "%{family[0]}:%{style[0]}:%{spacing}\n"],
            capture_output=True, text=True, check=True, timeout=10,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as e:
        raise FontListError(f"fc-list failed: {e}") from e

    fonts: dict[str, Font] = {}
    for line in out.splitlines():
        family, style, spacing = (line.split(":") + ["", ""])[:3]
        font = fonts.setdefault(family, Font(family, style, spacing == "100"))
        font.weights.append(700 if "Bold" in style else 400)
    return sorted(fonts.values(), key=lambda f: (not f.monospace, f.family.lower()))


def save(fonts: list[Font], path: Path) -> int:
    data = [{"family": f.family, "mono": f.monospace} for f in fonts]
    path.write_text(json.dumps(data, indent=2))
    return len(data)


if __name__ == "__main__":
    mono = [f for f in installed_fonts() if f.monospace]
    print(f"{len(mono)} monospaced fonts, e.g. {mono[0].label() if mono else None}")
    print(save(mono, Path("/tmp/mono.json")), "saved;", SAMPLE, 0x2A, 3.14)
