import { seatName } from "../engine/names";
import { TileFace } from "./tile";
import type { Discard, Seat } from "@mjers/mjers/types";

interface PondProps {
    seat: Seat;
    discards: Discard[];
    /** The most recent discard on the table, outlined so nobody misses it. */
    latest: Discard | undefined;
}

/** The tiles one seat has thrown, in order. Tiles another player claimed stay but fade. */
export function Pond({ seat, discards, latest }: PondProps) {
    const mine = discards.filter((discard) => discard.seat === seat);

    return (
        <ol className="pond" aria-label={`${seatName(seat)} discards`}>
            {mine.map((discard, index) => (
                <li key={index} className="pond-tile" data-claimed={discard.claimed} data-latest={discard === latest}>
                    <TileFace tile={discard.tile} />
                </li>
            ))}
        </ol>
    );
}
