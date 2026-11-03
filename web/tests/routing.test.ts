import { parseSearch, stringifySearch, validatePracticeSearch, validateRoomSearch } from "../src/routing/search";
import assert from "node:assert/strict";
import test from "node:test";

test("practice links preserve the whole u64 seed instead of rounding through a number", () => {
    const seed = "18446744073709551615";
    const search = validatePracticeSearch(parseSearch(`?seed=${seed}`));
    assert.equal(search.seed, seed);
    assert.equal(BigInt(search.seed!), (1n << 64n) - 1n);
    assert.equal(stringifySearch(search), `?seed=${seed}`);
});

test("room search normalizes invite codes and does not carry private query parameters", () => {
    const search = validateRoomSearch(parseSearch("?room=%20abc234%20&token=private&password=private"));
    assert.deepEqual(search, { room: "ABC234" });
    assert.equal(stringifySearch(search), "?room=ABC234");
});

test("invalid seeds and empty room codes fall back to normal entry or random practice", () => {
    assert.deepEqual(validatePracticeSearch(parseSearch("?seed=invalid")), {});
    assert.deepEqual(validatePracticeSearch(parseSearch("?seed=-1")), {});
    assert.deepEqual(validateRoomSearch(parseSearch("?room=%20")), {});
    assert.equal(stringifySearch({ room: undefined, seed: undefined }), "");
});
