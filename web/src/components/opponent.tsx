import { seatName, windName } from "../engine/names";
import { meldTiles } from "../engine/tiles";
import { TileBack, TileFace } from "./tile";
import type { Player, Seat } from "@mjers/mjers/types";

interface OpponentProps {
    player: Player;
    dealer: Seat;
    acting: boolean;
    /** Left and right opponents stack their tiles in a column. */
    side?: boolean;
}

/** One bot: its name, a back for each concealed tile, and everything it has shown. */
export function Opponent({ player, dealer, acting, side }: OpponentProps) {
    const shown = player.melds.map((meld, index) => ({ key: index, tiles: meldTiles(meld) }));

    return (
        <section className={side ? "opponent opponent-side" : "opponent"} aria-label={seatName(player.seat)} data-acting={acting}>
            <p className="seat-name">
                {seatName(player.seat)}
                <span>{windName(player.seat, dealer)}</span>
                {player.seat === dealer && <span>Dealer</span>}
            </p>
            <div className="concealed" aria-label={`${player.concealed_count} concealed tiles`}>
                {Array.from({ length: player.concealed_count }, (_, index) => (
                    <TileBack key={index} side={side} />
                ))}
            </div>
            <div className="shown">
                {shown.map(({ key, tiles }) => (
                    <div className="meld" key={key}>
                        {tiles.map((tile, index) => (
                            <TileFace key={index} tile={tile} />
                        ))}
                    </div>
                ))}
                {player.bonus.length > 0 && (
                    <div className="meld bonus">
                        {player.bonus.map((tile) => (
                            <TileFace key={tile} tile={tile} />
                        ))}
                    </div>
                )}
            </div>
        </section>
    );
}
