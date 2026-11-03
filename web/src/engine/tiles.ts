import type { Meld, Tile, TileCounts } from "@mjers/mjers/types";

export function parseTile(tile: Tile): { rank: number; suit: string } {
    return { rank: Number(tile.slice(0, -1)), suit: tile.slice(-1) };
}

/** Expands grouped notation such as "11567m188p" into one entry per tile, in written order. */
export function expandCounts(counts: TileCounts): Tile[] {
    const tiles: Tile[] = [];
    let ranks: string[] = [];

    for (const character of counts) {
        if (/\d/.test(character)) {
            ranks.push(character);
        } else {
            tiles.push(...ranks.map((rank) => `${rank}${character}`));
            ranks = [];
        }
    }

    return tiles;
}

/** The tiles a meld shows on the table. A chow is named by its lowest tile. */
export function meldTiles(meld: Meld): Tile[] {
    if ("chow" in meld) {
        const { rank, suit } = parseTile(meld.chow);

        return [0, 1, 2].map((offset) => `${rank + offset}${suit}`);
    }

    if ("pong" in meld) {
        return [meld.pong, meld.pong, meld.pong];
    }

    return Array.from({ length: 4 }, () => meld.kong[0]);
}

export function removeOne(tiles: Tile[], tile: Tile): Tile[] {
    const index = tiles.lastIndexOf(tile);

    return index === -1 ? tiles : [...tiles.slice(0, index), ...tiles.slice(index + 1)];
}
