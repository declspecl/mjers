/** A small seeded generator (mulberry32) for table cosmetics: hand order, discard scatter, dice. Never used for game logic. */
export function nextRandom(state: { random: number }): number {
    state.random = Math.trunc((state.random + 0x6d_2b_79_f5) % 4_294_967_296);

    let value = state.random;

    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);

    return ((value ^ (value >>> 14)) >>> 0) / 4_294_967_296;
}

export function randomInt(state: { random: number }, below: number): number {
    return Math.floor(nextRandom(state) * below);
}

export function shuffle<T>(state: { random: number }, items: T[]): T[] {
    const result = [...items];

    for (let index = result.length - 1; index > 0; index--) {
        const other = randomInt(state, index + 1);
        const held = result[index]!;

        result[index] = result[other]!;
        result[other] = held;
    }

    return result;
}
