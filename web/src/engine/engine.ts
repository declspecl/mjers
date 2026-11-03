import createMjers from "@mjers/mjers";

export type Mjers = Awaited<ReturnType<typeof createMjers>>;

/** The shared WebAssembly engine; the first call instantiates it. */
export function loadEngine(): Promise<Mjers> {
    return createMjers();
}
