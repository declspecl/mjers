"""Typed decision shards; file decoding and validation stay at this boundary."""

from collections.abc import Sequence
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


def concatenate(datasets: Sequence[DecisionDataset]) -> DecisionDataset:
    """Join shards collected under one encoding, ruleset, and teacher."""
    if not datasets:
        raise ValueError("concatenation requires datasets")
    first = datasets[0]
    if any(dataset.metadata != first.metadata for dataset in datasets):
        raise ValueError("concatenated datasets must share encoding, rules, and teacher")
    seeds = [set(np.unique(dataset.seeds).tolist()) for dataset in datasets]
    if sum(len(shard) for shard in seeds) != len(set().union(*seeds)):
        raise ValueError("concatenated datasets repeat hand seeds")
    if len(datasets) == 1:
        return first
    return DecisionDataset(
        metadata=first.metadata,
        tiles=np.concatenate([dataset.tiles for dataset in datasets]),
        context=np.concatenate([dataset.context for dataset in datasets]),
        action_mask=np.concatenate([dataset.action_mask for dataset in datasets]),
        actions=np.concatenate([dataset.actions for dataset in datasets]),
        returns=np.concatenate([dataset.returns for dataset in datasets]),
        seats=np.concatenate([dataset.seats for dataset in datasets]),
        seeds=np.concatenate([dataset.seeds for dataset in datasets]),
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


def load_datasets(paths: Sequence[Path]) -> DecisionDataset:
    """Load and join shards like `concatenate`, holding one shard beyond the result.

    Seeds and metadata are checked first from the small arrays; each field is
    then allocated once and filled shard by shard.
    """
    if not paths:
        raise ValueError("loading requires dataset paths")
    if len(paths) == 1:
        return load_dataset(paths[0])
    lengths: list[int] = []
    seeds: list[set[int]] = []
    metadata: list[DatasetMetadata] = []
    for path in paths:
        with np.load(path, allow_pickle=False) as archive:
            shard_seeds: NDArray[np.generic] = archive["seeds"]
            lengths.append(len(shard_seeds))
            seeds.append(set(np.unique(shard_seeds).tolist()))
            metadata.append(msgspec.json.decode(str(np.asarray(archive["metadata"], dtype=np.str_).item()), type=DatasetMetadata))
    if any(m != metadata[0] for m in metadata):
        raise ValueError("concatenated datasets must share encoding, rules, and teacher")
    if sum(len(shard) for shard in seeds) != len(set().union(*seeds)):
        raise ValueError("concatenated datasets repeat hand seeds")
    total = sum(lengths)
    tiles = np.empty((total, TILE_PLANES, 34), dtype=np.float32)
    context = np.empty((total, CONTEXT_SIZE), dtype=np.float32)
    action_mask = np.empty((total, ACTION_COUNT), dtype=np.bool_)
    actions = np.empty(total, dtype=np.int64)
    returns = np.empty(total, dtype=np.float32)
    seats = np.empty(total, dtype=np.uint8)
    hand_seeds = np.empty(total, dtype=np.uint64)
    start = 0
    for path, length in zip(paths, lengths, strict=True):
        shard = load_dataset(path)
        if len(shard.actions) != length:
            raise ValueError(f"{path} changed while loading")
        end = start + length
        tiles[start:end] = shard.tiles
        context[start:end] = shard.context
        action_mask[start:end] = shard.action_mask
        actions[start:end] = shard.actions
        returns[start:end] = shard.returns
        seats[start:end] = shard.seats
        hand_seeds[start:end] = shard.seeds
        start = end
    return DecisionDataset(metadata[0], tiles, context, action_mask, actions, returns, seats, hand_seeds)
