import { seatName, tileName, windName } from "../engine/names";
import type { TableState } from "../engine/use-table";
import { type Pose, STAGE_HEIGHT, STAGE_WIDTH, TILE_HEIGHT, TILE_WIDTH, toStage } from "../table/geometry";
import { layout, rackLayout } from "../table/layout";
import { placedIds, SEATS } from "../table/model";
import { Dice } from "./dice";
import { tileArtwork } from "./tile";
import type { Seat } from "@mjers/mjers/types";
import {
    type KeyboardEvent,
    type PointerEvent,
    type ReactNode,
    useCallback,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
} from "react";

const MOVE_MS = 280;
const DRAG_THRESHOLD = 6;

interface Drag {
    id: number;
    startX: number;
    startY: number;
    dx: number;
    dy: number;
    moved: boolean;
}

interface StageProps {
    table: TableState;
    analytic: boolean;
    selected: number | null;
    /** A tile the hint suggests discarding. */
    suggested: number | null;
    canDiscard: (id: number) => boolean;
    onSelect: (id: number | null) => void;
    onDiscard: (id: number) => void;
    onDraw: () => void;
    onMove: (id: number, index: number) => void;
    onOpeningDone: () => void;
    children?: ReactNode;
}

/** Fits the fixed-size table into whatever room it has. */
function useFit(box: React.RefObject<HTMLDivElement | null>): number {
    const [scale, setScale] = useState(0.5);

    useLayoutEffect(() => {
        const element = box.current;

        if (!element) {
            return;
        }

        const measure = () => {
            const { width, height } = element.getBoundingClientRect();

            setScale(Math.max(0.2, Math.min(width / STAGE_WIDTH, height / STAGE_HEIGHT)));
        };
        const observer = new ResizeObserver(measure);

        measure();
        observer.observe(element);

        return () => observer.disconnect();
    }, [box]);

    return scale;
}

/** Where a seat's name sits: in the strip just inside its hand, where discards never land. Side seats' names run along their edge. */
function labelPlace(seat: Seat): { left: number; top: number; transform: string } {
    const point = toStage(seat, 0, 400);
    const turn = seat === 1 ? -90 : seat === 3 ? 90 : 0;

    return { left: point.x, top: point.y, transform: `translate(-50%, -50%) rotate(${turn}deg)` };
}

function transformOf(pose: Pose): string {
    return `translate(${pose.x - TILE_WIDTH / 2}px, ${pose.y - TILE_HEIGHT / 2}px) rotate(${pose.rotation}deg) scale(${pose.scale})`;
}

/** The table: walls, hands, discards and dice, every tile a single element that moves between places. */
export function Stage(props: StageProps) {
    const { table, analytic, selected, suggested, canDiscard, onSelect, onDiscard, onDraw, onMove, onOpeningDone, children } = props;
    const { model, phase, frames, observation } = table;
    const box = useRef<HTMLDivElement>(null);
    const scale = useFit(box);
    const [drag, setDrag] = useState<Drag | null>(null);
    const [cursor, setCursor] = useState({ hand: -1, index: 0 });
    const frameIndex = cursor.hand === table.hand ? cursor.index : 0;
    const frame = phase === "opening" ? frames[frameIndex] : undefined;
    const pendingFirst = model.pending[0]?.id ?? null;
    const offered =
        phase === "play" && table.humanToAct && observation.decision.kind === "claim" && observation.decision.offer === "discard"
            ? (model.discardIds.at(-1) ?? null)
            : null;

    useEffect(() => {
        if (phase !== "opening") {
            return;
        }

        if (!frame) {
            onOpeningDone();

            return;
        }

        const timer = setTimeout(() => setCursor({ hand: table.hand, index: frameIndex + 1 }), frame.hold);

        return () => clearTimeout(timer);
    }, [phase, frame, frameIndex, table.hand, onOpeningDone]);

    const base = useMemo(() => rackLayout(model, { order: model.hands[0]!, drawn: model.drawn[0] ?? null, selected }), [model, selected]);

    const preview = useMemo(() => {
        const hand = model.hands[0]!;
        const drawn = model.drawn[0] ?? null;

        if (!drag?.moved) {
            return { order: hand, drawn };
        }

        const slot = base.slots.find((entry) => entry.id === drag.id);
        const pointer = (slot?.x ?? 0) + drag.dx / scale;
        const others = hand.filter((id) => id !== drag.id);
        const index = others.filter((id) => (base.slots.find((entry) => entry.id === id)?.x ?? 0) < pointer).length;

        return { order: [...others.slice(0, index), drag.id, ...others.slice(index)], drawn: drawn === drag.id ? null : drawn, index };
    }, [model, base, drag, scale]);

    const poses = useMemo(() => {
        if (frame) {
            return frame.poses;
        }

        const placed = layout(model, { order: preview.order, drawn: preview.drawn, selected: drag?.moved ? null : selected });

        if (drag?.moved) {
            const start = base.poses.get(drag.id);

            if (start) {
                placed[drag.id] = { ...start, x: start.x + drag.dx / scale, y: start.y + drag.dy / scale - 10, z: 9000 };
            }
        }

        return placed;
    }, [frame, model, preview, selected, drag, base, scale]);

    const rack = useMemo(
        () => new Set(frame ? [] : [...model.hands[0]!, ...(model.drawn[0] === null ? [] : [model.drawn[0]!])]),
        [frame, model],
    );
    const placed = useMemo(() => placedIds(model), [model]);
    const rackOrder = useMemo(() => base.slots.map((slot) => slot.id), [base]);

    const finishDrag = useCallback(
        (event: PointerEvent<HTMLDivElement>) => {
            if (!drag) {
                return;
            }

            event.currentTarget.releasePointerCapture?.(event.pointerId);
            setDrag(null);

            if (drag.moved) {
                if (preview.index !== undefined) {
                    onMove(drag.id, preview.index);
                }

                return;
            }

            if (selected === drag.id && canDiscard(drag.id)) {
                onDiscard(drag.id);
            } else {
                onSelect(selected === drag.id ? null : drag.id);
            }
        },
        [drag, preview, selected, canDiscard, onMove, onDiscard, onSelect],
    );

    const onKey = (event: KeyboardEvent<HTMLDivElement>, id: number) => {
        const position = rackOrder.indexOf(id);
        const step = event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : 0;

        if (step !== 0) {
            event.preventDefault();

            if (event.shiftKey) {
                const handIndex = model.hands[0]!.indexOf(id);

                onMove(id, handIndex === -1 ? model.hands[0]!.length + (step < 0 ? -1 : 0) : Math.max(0, handIndex + step));
                onSelect(id);

                return;
            }

            const neighbour = rackOrder[Math.max(0, Math.min(rackOrder.length - 1, position + step))];

            if (neighbour !== undefined) {
                onSelect(neighbour);
                box.current?.querySelector<HTMLElement>(`[data-id="${neighbour}"]`)?.focus();
            }
        }

        if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();

            if (selected === id && canDiscard(id)) {
                onDiscard(id);
            } else {
                onSelect(id);
            }
        }
    };

    const focusable = selected !== null && rack.has(selected) ? selected : rackOrder[0];
    const acting = new Set(observation.to_act);

    return (
        <div className="stage-box" ref={box}>
            <div className="stage-frame" style={{ width: STAGE_WIDTH * scale, height: STAGE_HEIGHT * scale }}>
                <div className="stage" style={{ transform: `scale(${scale})` }} data-phase={phase}>
                    <div className="felt" aria-hidden="true" />
                    {phase !== "opening" &&
                        SEATS.map((seat) => {
                            return (
                                <p
                                    key={seat}
                                    className="seat-label"
                                    style={labelPlace(seat)}
                                    data-acting={phase === "play" && acting.has(seat)}
                                    data-dealer={seat === table.dealer}
                                >
                                    {analytic && <span>{seatName(seat)}</span>}
                                    {windName(seat, table.dealer)}
                                </p>
                            );
                        })}
                    {frame?.dealer !== undefined && (
                        <p className="seat-label" style={labelPlace(frame.dealer)} data-dealer="true">
                            East
                        </p>
                    )}
                    {frame?.dice && <Dice values={frame.dice.values} rolling={frame.dice.rolling} roller={frame.dice.roller} />}
                    {poses.map((pose, id) => {
                        const face = pose.face === "up" ? model.faces[id] : null;
                        const inRack = rack.has(id);
                        const drawable = !frame && pendingFirst !== null && !placed.has(id);
                        const dragging = drag?.moved && drag.id === id;

                        return (
                            <div
                                key={id}
                                data-id={id}
                                className="tile"
                                data-face={face ? "up" : pose.face === "standing" ? "standing" : "down"}
                                data-next={id === pendingFirst}
                                data-offered={id === offered}
                                data-selected={inRack && id === selected}
                                data-suggested={inRack && id === suggested}
                                data-dragging={dragging}
                                data-drawable={drawable}
                                style={{
                                    transform: transformOf(pose),
                                    zIndex: pose.z,
                                    transitionDuration: dragging ? "0ms" : `${frame ? frame.duration : MOVE_MS}ms`,
                                    transitionDelay: frame?.delays ? `${frame.delays[id] ?? 0}ms` : "0ms",
                                }}
                                role={inRack || id === pendingFirst ? "button" : undefined}
                                tabIndex={inRack ? (id === focusable ? 0 : -1) : id === pendingFirst ? 0 : undefined}
                                aria-label={
                                    inRack && face
                                        ? `${tileName(face)}${id === selected ? ", selected" : ""}`
                                        : id === pendingFirst
                                          ? "Draw tile"
                                          : undefined
                                }
                                aria-hidden={inRack || id === pendingFirst ? undefined : true}
                                onPointerDown={
                                    inRack
                                        ? (event) => {
                                              event.currentTarget.setPointerCapture?.(event.pointerId);
                                              setDrag({ id, startX: event.clientX, startY: event.clientY, dx: 0, dy: 0, moved: false });
                                          }
                                        : undefined
                                }
                                onPointerMove={
                                    inRack
                                        ? (event) => {
                                              if (drag?.id !== id) {
                                                  return;
                                              }

                                              const dx = event.clientX - drag.startX;
                                              const dy = event.clientY - drag.startY;

                                              setDrag({ ...drag, dx, dy, moved: drag.moved || Math.hypot(dx, dy) > DRAG_THRESHOLD });
                                          }
                                        : undefined
                                }
                                onPointerUp={inRack ? finishDrag : undefined}
                                onPointerCancel={inRack ? () => setDrag(null) : undefined}
                                onClick={drawable ? onDraw : undefined}
                                onKeyDown={
                                    inRack
                                        ? (event) => onKey(event, id)
                                        : id === pendingFirst
                                          ? (event) => {
                                                if (event.key === "Enter" || event.key === " ") {
                                                    event.preventDefault();
                                                    onDraw();
                                                }
                                            }
                                          : undefined
                                }
                            >
                                {face ? (
                                    <img key="face" src={tileArtwork(face)} alt="" draggable={false} />
                                ) : (
                                    <span key="back" className="tile-back" />
                                )}
                            </div>
                        );
                    })}
                </div>
                {children}
            </div>
        </div>
    );
}
