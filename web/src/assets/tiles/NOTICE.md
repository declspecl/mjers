# Tile artwork

The 42 tile faces in this folder are from Anisimova's Mahjong game pack by Anisimova,
https://anisimova.itch.io/anisimovas-mahjong-game-pack (itch.io, pay what you want).

The author's terms on that page: "you can use the files in your own projects and modify them as you wish. My only request is that
you do not resell my files."

The pack ships as one Adobe Illustrator EPS sheet. Each tile was rendered from that sheet to a transparent PNG at 232 by 305 pixels, with the
same crop window on every tile. The pack's soft shadow had a white background baked in, so it was converted to a transparent shadow, and the tile outline was kept opaque. The faces, glyphs and colors are unchanged. Do not sell these files on their own.

File names are `<kind>-<rank>.png`: `man`, `pin` and `sou` run 1 to 9, `honor` runs 1 to 7 in the engine's order (East, South, West, North,
White, Green, Red), and `flower` and `season` run 1 to 4.

To regenerate them from the EPS: `python3 scripts/make-tiles.py "/path/to/Mahjong.eps" src/assets/tiles` (needs Ghostscript and poppler).
