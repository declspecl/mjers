"""Typed decision shards; file decoding and validation stay at this boundary."""

from dataclasses import dataclass
from pathlib import Path

import msgspec
import numpy as np
from numpy import savez_compressed
from numpy.typing import NDArray

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, ENCODING_VERSION, TILE_PLANES


@dataclass(frozen=True)
class DatasetMetadata:
    encoding_version: int
    rules_json: str
    teacher_json: str


@dataclass(frozen=True)
class DecisionDataset:
    metadata: DatasetMetadata
    tiles: NDArray[np.float32]
    context: NDArray[np.float32]
    action_mask: NDArray[np.bool_]
    actions: NDArray[np.int64]
    returns: NDArray[np.float32]
    seats: NDArray[np.uint8]
    seeds: NDArray[np.uint64]

    def __post_init__(self) -> None:
        if self.metadata.encoding_version != ENCODING_VERSION:
            raise ValueError("dataset encoding version does not match the engine")
        n = len(self.actions)
        if n == 0:
            raise ValueError("dataset must contain decisions")
        if self.tiles.shape != (n, TILE_PLANES, 34) or self.context.shape != (n, CONTEXT_SIZE):
            raise ValueError("dataset observation dimensions do not match the encoder")
        if self.action_mask.shape != (n, ACTION_COUNT):
            raise ValueError("dataset action masks must have one row per decision")
        if any(array.shape != (n,) for array in (self.actions, self.returns, self.seats, self.seeds)):
            raise ValueError("dataset labels, returns, seats, and seeds must align")
        if (self.actions < 0).any() or (self.actions >= ACTION_COUNT).any():
            raise ValueError("dataset contains an out-of-range action")
        if not self.action_mask[np.arange(n), self.actions].all():
            raise ValueError("dataset contains an illegal teacher action")
        if (self.seats >= 4).any():
            raise ValueError("dataset contains an invalid seat")
        if not all(np.isfinite(array).all() for array in (self.tiles, self.context, self.returns)):
            raise ValueError("dataset contains non-finite values")


def save_dataset(path: Path, dataset: DecisionDataset) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as output:
        savez_compressed(
            output,
            metadata=np.asarray(msgspec.json.encode(dataset.metadata).decode()),
            tiles=dataset.tiles,
            context=dataset.context,
            action_mask=dataset.action_mask,
            actions=dataset.actions,
            returns=dataset.returns,
            seats=dataset.seats,
            seeds=dataset.seeds,
        )


def load_dataset(path: Path) -> DecisionDataset:
    with np.load(path, allow_pickle=False) as archive:
        # Check stored dtypes before conversion: floating-point action labels
        # must not silently truncate to integers at the file boundary.
        expected = {
            "tiles": np.dtype(np.float32),
            "context": np.dtype(np.float32),
            "action_mask": np.dtype(np.bool_),
            "actions": np.dtype(np.int64),
            "returns": np.dtype(np.float32),
            "seats": np.dtype(np.uint8),
            "seeds": np.dtype(np.uint64),
        }
        for name, dtype in expected.items():
            raw: NDArray[np.generic] = archive[name]
            if raw.dtype != dtype:
                raise ValueError(f"dataset {name} has dtype {raw.dtype}, expected {dtype}")
        metadata = msgspec.json.decode(str(np.asarray(archive["metadata"], dtype=np.str_).item()), type=DatasetMetadata)
        return DecisionDataset(
            metadata=metadata,
            tiles=np.asarray(archive["tiles"], dtype=np.float32),
            context=np.asarray(archive["context"], dtype=np.float32),
            action_mask=np.asarray(archive["action_mask"], dtype=np.bool_),
            actions=np.asarray(archive["actions"], dtype=np.int64),
            returns=np.asarray(archive["returns"], dtype=np.float32),
            seats=np.asarray(archive["seats"], dtype=np.uint8),
            seeds=np.asarray(archive["seeds"], dtype=np.uint64),
        )
