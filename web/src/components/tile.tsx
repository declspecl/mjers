import { tileName } from "../engine/names";
import { tileAssetName } from "../engine/tiles";
import type { Tile } from "@mjers/mjers/types";

const ARTWORK = import.meta.glob<string>("../assets/tiles/*.png", { eager: true, query: "?url", import: "default" });

export function tileArtwork(tile: Tile): string | undefined {
    return ARTWORK[`../assets/tiles/${tileAssetName(tile)}.png`];
}

/** One tile face, drawn at the width its container gives it. */
export function TileFace({ tile }: { tile: Tile }) {
    return <img className="tile-face" src={tileArtwork(tile)} alt={tileName(tile)} width={232} height={305} draggable={false} />;
}
