import { arrange } from "../src/table/arrangement";
import { TILE_HEIGHT, TILE_WIDTH } from "../src/table/geometry";
import { dropIndex, type RackContents, rackLayout } from "../src/table/layout";
import assert from "node:assert/strict";
import test from "node:test";

const SCREENS = {
    "small phone portrait": [375, 667],
    "phone portrait": [390, 844],
    "large phone portrait": [430, 932],
    "phone portrait in a browser": [390, 664],
    "phone landscape": [844, 390],
    "large phone landscape": [932, 430],
    "tablet portrait": [820, 1180],
    "tablet landscape": [1180, 820],
    "laptop with analysis": [940, 800],
    desktop: [1440, 900],
} as const;

const HAND = Array.from({ length: 13 }, (_, index) => index);

/** Only your melds and flowers are read from the model when laying out the rack. */
function rackModel(melds: number[][] = [], bonus: number[] = []): RackContents {
    return {
        melds: [melds.map((ids) => ({ ids, meld: { pong: "1m" } })), [], [], []],
        bonus: [bonus, [], [], []],
    };
}

interface Rect {
    left: number;
    top: number;
    right: number;
    bottom: number;
}

/** A tile's box on screen, in CSS pixels. */
function screenRect(arrangement: ReturnType<typeof arrange>, pose: { x: number; y: number; scale: number }): Rect {
    const { scale, table } = arrangement;
    const halfWidth = (TILE_WIDTH * pose.scale * scale) / 2;
    const halfHeight = (TILE_HEIGHT * pose.scale * scale) / 2;
    const x = table.x + pose.x * scale;
    const y = table.y + pose.y * scale;

    return { left: x - halfWidth, top: y - halfHeight, right: x + halfWidth, bottom: y + halfHeight };
}

for (const [name, [width, height]] of Object.entries(SCREENS)) {
    test(`${name}: the hand fits on screen, clear of the table, and is big enough to tap`, () => {
        const arrangement = arrange(width, height);
        const { table } = arrangement;
        const { poses } = rackLayout(rackModel([[20, 21, 22]], [30, 31]), {
            order: HAND,
            drawn: 13,
            selected: null,
            area: arrangement.rack,
        });

        assert.ok(table.x >= 0 && table.y >= 0, "the table starts on screen");
        assert.ok(table.x + table.size <= width + 0.5 && table.y + table.size <= height + 0.5, "the table ends on screen");

        for (const id of [...HAND, 13, 20, 21, 22, 30, 31]) {
            const rect = screenRect(arrangement, poses.get(id)!);

            assert.ok(rect.left >= -0.5 && rect.right <= width + 0.5, `tile ${id} fits across`);
            assert.ok(rect.top >= -0.5 && rect.bottom <= height + 0.5, `tile ${id} fits down`);

            const overlapsTable =
                rect.right > table.x && rect.left < table.x + table.size && rect.bottom > table.y && rect.top < table.y + table.size;

            assert.ok(!overlapsTable || rect.top >= table.y + table.size - 1, `tile ${id} stays off the felt`);
        }

        const handTile = screenRect(arrangement, poses.get(0)!);

        assert.ok(handTile.right - handTile.left >= 30, `hand tiles are ${(handTile.right - handTile.left).toFixed(1)}px wide`);
    });
}

test("the table is never smaller than the old fixed layout made it, and fills the width of a portrait phone", () => {
    // The laptop with the analysis panel gives a little of its table to a bar for the corner controls, so they stay off the felt.
    const screens: (readonly [number, number])[] = [
        ...Object.entries(SCREENS)
            .filter(([name]) => name !== "laptop with analysis")
            .map(([, size]) => size),
        [375, 553],
        [430, 740],
        [360, 640],
    ];

    for (const [width, height] of screens) {
        const { table } = arrange(width, height);
        // The old stage was a 1000 by 1160 canvas, table and rack together, inside 8px of padding.
        const old = Math.min((width - 16) / 1000, (height - 16) / 1160) * 1000;

        assert.ok(table.size >= old - 0.5, `${width}x${height}: table ${table.size.toFixed(0)}px, was ${old.toFixed(0)}px`);
    }

    const portraitPhones: (readonly [number, number])[] = [
        [375, 667],
        [390, 664],
        [390, 844],
        [430, 932],
    ];

    for (const [width, height] of portraitPhones) {
        assert.equal(arrange(width, height).table.size, width, `${width}x${height}`);
    }
});

test("a narrow tall screen splits the hand over two rows, with the drawn tile ending the lower one", () => {
    const arrangement = arrange(390, 844);
    const { slots } = rackLayout(rackModel(), { order: HAND, drawn: 13, selected: null, area: arrangement.rack });

    assert.equal(arrangement.rack.hand.rows, 2);
    assert.deepEqual(
        slots.filter((slot) => slot.row === 0).map((slot) => slot.id),
        [0, 1, 2, 3, 4, 5, 6],
    );
    assert.deepEqual(
        slots.filter((slot) => slot.row === 1).map((slot) => slot.id),
        [7, 8, 9, 10, 11, 12, 13],
    );
});

test("wide screens keep a single row and move the buttons into the margin when it has room", () => {
    assert.equal(arrange(1180, 820).rack.hand.rows, 1);
    assert.equal(arrange(1180, 820).actions.place, "margin");
    assert.equal(arrange(940, 800).actions.place, "felt");
    assert.equal(arrange(390, 844).actions.place, "band");
});

test("a dragged tile drops into the row nearest the pointer", () => {
    const arrangement = arrange(390, 844);
    const { slots } = rackLayout(rackModel(), { order: HAND, drawn: null, selected: null, area: arrangement.rack });
    const slot = (id: number) => slots.find((entry) => entry.id === id)!;

    // Dragging tile 0 to just right of tile 9 in the lower row puts it after 7, 8 and 9.
    assert.equal(dropIndex(slots, HAND, 0, { x: slot(9).x + 1, y: slot(9).y }), 9);
    // Dragging tile 12 to the far left of the upper row puts it first.
    assert.equal(dropIndex(slots, HAND, 12, { x: slot(0).x - 50, y: slot(0).y }), 0);
});
