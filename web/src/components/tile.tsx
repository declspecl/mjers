import { tileName } from "../engine/names";
import { tileAssetName } from "../engine/tiles";
import type { Tile } from "@mjers/mjers/types";

const ARTWORK = import.meta.glob<string>("../assets/tiles/*.png", { eager: true, query: "?url", import: "default" });

/** One tile face, drawn at the width its container gives it. */
export function TileFace({ tile }: { tile: Tile }) {
    const source = ARTWORK[`../assets/tiles/${tileAssetName(tile)}.png`];

    return <img className="tile-face" src={source} alt={tileName(tile)} width={232} height={305} draggable={false} />;
}

/** A concealed tile seen from the other side of the table. */
export function TileBack({ side }: { side?: boolean }) {
    return <span className={side ? "tile-back tile-back-side" : "tile-back"} aria-hidden="true" />;
}
