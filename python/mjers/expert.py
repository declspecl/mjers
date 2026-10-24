"""Label a policy's unsure decisions by paired playouts, then fine-tune toward them.

`collect` plays one student seat per hand (the policy's best action) against
the field. Where the favourite has less than `--confident` probability, the
policy's two most likely actions are played out on the same sampled deals
(`Env.estimate_actions`: the policy in its seat, the field elsewhere), and the
second's paired lead over the favourite is stored with its standard error. A
few other student decisions per hand are kept as anchors.

`refine` starts from the policy and distills its own move probabilities on
those positions, except that at searched ones the two candidates' combined
probability is split by the posterior chance that the second is better. With
a Gaussian prior of standard deviation `--prior` on true leads and an observed
lead d with error e, that chance is Φ(d·τ / (e·√(τ² + e²))).
"""

import argparse
import math
import multiprocessing
from collections.abc import Callable, Sequence
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any

import msgspec
import numpy as np
import torch
from numpy import savez_compressed
from numpy.typing import NDArray
from torch import Tensor, from_numpy

from mjers._native import ACTION_COUNT, CONTEXT_SIZE, ENCODING_VERSION, TILE_PLANES, Env
from mjers.collect import make_env
from mjers.model import DeviceName, ResidualPolicyNet, TrainedPolicy, hand_seeds, load_policy, native_network, save_policy
from mjers.train import PolicyBatch, learn_batch, probabilities


@dataclass(frozen=True)
class SearchMetadata:
    encoding_version: int
    rules_json: str
    field_json: str
    student: str
    samples: int
    confident: float


@dataclass(frozen=True)
class SearchedPositions:
    metadata: SearchMetadata
    tiles: NDArray[np.float32]
    context: NDArray[np.float32]
    action_mask: NDArray[np.bool_]
    # The policy's two most likely actions, favourite first.
    candidates: NDArray[np.int64]
    # False for anchors, whose lead and error are zero.
    searched: NDArray[np.bool_]
    leads: NDArray[np.float32]
    lead_errors: NDArray[np.float32]
    seeds: NDArray[np.uint64]

    def __post_init__(self) -> None:
        if self.metadata.encoding_version != ENCODING_VERSION:
            raise ValueError("positions were encoded for another engine version")
        n = len(self.seeds)
        if self.tiles.shape != (n, TILE_PLANES, 34) or self.context.shape != (n, CONTEXT_SIZE):
            raise ValueError("position observations do not match the encoder")
        if self.action_mask.shape != (n, ACTION_COUNT) or self.candidates.shape != (n, 2):
            raise ValueError("positions need one mask and two candidates per row")
        if any(array.shape != (n,) for array in (self.searched, self.leads, self.lead_errors)):
            raise ValueError("position labels must align")
        if n and not self.action_mask[np.arange(n)[:, None], self.candidates].all():
            raise ValueError("positions contain an illegal candidate")
        if (self.lead_errors < 0).any() or not np.isfinite(self.leads).all():
            raise ValueError("positions contain invalid playout estimates")


def save_positions(path: Path, positions: SearchedPositions) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as output:
        savez_compressed(
            output,
            metadata=np.asarray(msgspec.json.encode(positions.metadata).decode()),
            tiles=positions.tiles,
            context=positions.context,
            action_mask=positions.action_mask,
            candidates=positions.candidates,
            searched=positions.searched,
            leads=positions.leads,
            lead_errors=positions.lead_errors,
            seeds=positions.seeds,
        )


def load_positions(path: Path) -> SearchedPositions:
    with np.load(path, allow_pickle=False) as data:
        return SearchedPositions(
            metadata=msgspec.json.decode(str(data["metadata"]), type=SearchMetadata),
            tiles=data["tiles"],
            context=data["context"],
            action_mask=data["action_mask"],
            candidates=data["candidates"],
            searched=data["searched"],
            leads=data["leads"],
            lead_errors=data["lead_errors"],
            seeds=data["seeds"],
        )


def concatenate(parts: Sequence[SearchedPositions]) -> SearchedPositions:
    if not parts:
        raise ValueError("concatenation requires positions")
    if any(part.metadata != parts[0].metadata for part in parts):
        raise ValueError("concatenated positions must share their collection settings")
    seeds = [set(np.unique(part.seeds).tolist()) for part in parts]
    if sum(len(shard) for shard in seeds) != len(set().union(*seeds)):
        raise ValueError("concatenated positions repeat hand seeds")

    def join(name: str) -> NDArray[Any]:
        return np.concatenate([getattr(part, name) for part in parts])

    return SearchedPositions(
        parts[0].metadata,
        join("tiles"),
        join("context"),
        join("action_mask"),
        join("candidates"),
        join("searched"),
        join("leads"),
        join("lead_errors"),
        join("seeds"),
    )


@dataclass(frozen=True)
class CollectionConfig:
    samples: int
    confident: float
    anchors_per_hand: int
    seed: int

    def __post_init__(self) -> None:
        if self.samples <= 1 or self.anchors_per_hand < 0:
            raise ValueError("playouts need at least two samples and anchors cannot be negative")
        if not 0 < self.confident <= 1:
            raise ValueError("the confidence threshold must be in (0, 1]")


def collect_positions(env: Env, student: TrainedPolicy, student_path: str, seeds: range, config: CollectionConfig) -> SearchedPositions:
    """Searched and anchor positions from hands with one student seat each."""
    if student.metadata.dataset.rules_json != env.rules_json:
        raise ValueError("the student was trained under different rules")
    if not isinstance(student.network, ResidualPolicyNet):
        raise ValueError("only suit-resnet-v2 students can be searched")
    native = native_network(student.network)
    rows: list[tuple[NDArray[np.float32], NDArray[np.float32], NDArray[np.bool_], int, int, bool, float, float, int]] = []
    for seed in seeds:
        rng = np.random.default_rng([config.seed, seed])
        seat = int(rng.integers(4))
        observation = env.reset(seed, dealer=seed % 4)
        unsearched: list[int] = []
        while observation is not None:
            if observation.seat != seat:
                observation = env.step(env.teacher_action()).observation
                continue
            logits = np.asarray(env.evaluate_network(native)[0], dtype=np.float64)
            # Stable sorting puts the lower index first on ties, as the Rust bots do.
            first, second = (int(index) for index in np.argsort(-logits, kind="stable")[:2])
            legal = observation.action_mask
            if legal.sum() > 1:
                exp = np.exp(logits[legal] - logits[first])
                searched = exp.max() / exp.sum() < config.confident
                lead, error = 0.0, 0.0
                if searched:
                    estimate_seed = int(rng.integers(2**63))
                    _, (lead, error) = env.estimate_actions(native, [first, second], samples=config.samples, seed=estimate_seed)
                else:
                    unsearched.append(len(rows))
                rows.append((observation.tiles, observation.context, legal, first, second, bool(searched), lead, error, seed))
            observation = env.step(first).observation
        # Keep only a few anchors per hand.
        keep = set(rng.permutation(unsearched)[: config.anchors_per_hand].tolist()) if unsearched else set()
        for index in reversed(unsearched):
            if index not in keep:
                del rows[index]
    metadata = SearchMetadata(ENCODING_VERSION, env.rules_json, env.teacher_json, student_path, config.samples, config.confident)
    if not rows:
        raise ValueError("no student decisions were collected")
    return SearchedPositions(
        metadata,
        np.stack([row[0] for row in rows]),
        np.stack([row[1] for row in rows]),
        np.stack([row[2] for row in rows]),
        np.asarray([[row[3], row[4]] for row in rows], dtype=np.int64),
        np.asarray([row[5] for row in rows], dtype=np.bool_),
        np.asarray([row[6] for row in rows], dtype=np.float32),
        np.asarray([row[7] for row in rows], dtype=np.float32),
        np.asarray([row[8] for row in rows], dtype=np.uint64),
    )


@dataclass(frozen=True)
class CollectionJob:
    student: str
    field: str
    rules: str
    seeds: range
    config: CollectionConfig


def run_job(job: CollectionJob) -> SearchedPositions:
    torch.set_num_threads(1)
    student = load_policy(Path(job.student), torch.device("cpu"))
    return collect_positions(make_env(job.field, job.rules), student, job.student, job.seeds, job.config)


def second_is_better(leads: Tensor, errors: Tensor, prior: float) -> Tensor:
    """Posterior probability that the true lead is positive under a N(0, prior²) prior."""
    z = leads * prior / (errors * torch.sqrt(prior**2 + errors**2))
    exact = torch.where(leads > 0, 1.0, torch.where(leads < 0, 0.0, 0.5))
    return torch.where(errors > 0, 0.5 * (1 + torch.erf(z / math.sqrt(2))), exact)


def search_targets(base: Tensor, positions: SearchedPositions, prior: float) -> Tensor:
    """`base` probabilities with each searched row's candidate mass split by the posterior."""
    targets = base.clone()
    rows = torch.arange(len(base))
    candidates = from_numpy(positions.candidates)
    mass = base[rows, candidates[:, 0]] + base[rows, candidates[:, 1]]
    better = second_is_better(from_numpy(positions.leads).double(), from_numpy(positions.lead_errors).double(), prior).float()
    searched = from_numpy(positions.searched)
    targets[rows, candidates[:, 0]] = torch.where(searched, mass * (1 - better), base[rows, candidates[:, 0]])
    targets[rows, candidates[:, 1]] = torch.where(searched, mass * better, base[rows, candidates[:, 1]])
    return targets


@dataclass(frozen=True)
class RefineConfig:
    prior: float
    epochs: int
    batch_size: int
    learning_rate: float
    validation_every: int
    seed: int
    device: DeviceName

    def __post_init__(self) -> None:
        if min(self.epochs, self.batch_size, self.validation_every) <= 0:
            raise ValueError("epochs, batch size, and validation spacing must be positive")
        if not all(math.isfinite(value) and value > 0 for value in (self.prior, self.learning_rate)):
            raise ValueError("the prior and learning rate must be positive")


@dataclass(frozen=True)
class RefineMetrics:
    epoch: int
    # Soft cross-entropy on validation rows.
    searched_loss: float
    anchor_loss: float
    # Searched validation rows whose more likely candidate is the posterior's.
    agreement: float
    # Searched validation rows where the favourite is no longer the network's best.
    flipped: float


def evaluate(
    policy: TrainedPolicy, data: PolicyBatch, searched: Tensor, better: Tensor, candidates: Tensor, config: RefineConfig, epoch: int
) -> RefineMetrics:
    device = torch.device(config.device)
    policy.network.eval()
    losses: list[Tensor] = []
    preferred: list[Tensor] = []
    best: list[Tensor] = []
    with torch.inference_mode():
        for start in range(0, len(data.actions), config.batch_size):
            chunk = data.rows(start, start + config.batch_size).to(device)
            logits = policy.network.forward(chunk.tiles, chunk.context, chunk.action_mask)
            log_probs = logits.log_softmax(dim=1).masked_fill(~chunk.action_mask, 0.0)
            assert chunk.targets is not None
            losses.append(-(chunk.targets * log_probs).sum(dim=1).cpu())
            pair = candidates[start : start + config.batch_size].to(device)
            chosen = logits.gather(1, pair)
            preferred.append((chosen[:, 1] > chosen[:, 0]).cpu())
            best.append((logits.argmax(dim=1) != pair[:, 0]).cpu())
    loss, second, moved = torch.cat(losses), torch.cat(preferred), torch.cat(best)
    rows = searched.sum().clamp(min=1)
    return RefineMetrics(
        epoch,
        float(loss[searched].mean()) if searched.any() else 0.0,
        float(loss[~searched].mean()) if (~searched).any() else 0.0,
        float(((second == (better > 0.5)) & searched).sum() / rows),
        float((moved & searched).sum() / rows),
    )


def refine(
    policy: TrainedPolicy, positions: SearchedPositions, config: RefineConfig, report: Callable[[RefineMetrics], None]
) -> TrainedPolicy:
    """Fine-tunes `policy` in place toward the search targets; the value head is not trained."""
    if policy.metadata.dataset.rules_json != positions.metadata.rules_json:
        raise ValueError("the positions were collected under different rules")
    seeds = set(hand_seeds(positions.seeds))
    if seeds & set(policy.metadata.validation_seeds):
        raise ValueError("searched hands overlap the policy's validation hands")
    device = torch.device(config.device)
    torch.manual_seed(config.seed)
    n = len(positions.seeds)
    host = torch.device("cpu")
    batch = PolicyBatch(
        from_numpy(positions.tiles),
        from_numpy(positions.context),
        from_numpy(positions.action_mask),
        from_numpy(positions.candidates[:, 0]),
        torch.zeros(n),
    )
    base = probabilities(policy, batch, config.batch_size, device)
    targets = search_targets(base, positions, config.prior)
    batch = replace(batch, actions=targets.argmax(dim=1), targets=targets).to(host)
    validation_rows = from_numpy(positions.seeds % config.validation_every == 0)
    training = batch.select(torch.nonzero(~validation_rows).squeeze(1))
    validation = batch.select(torch.nonzero(validation_rows).squeeze(1))
    better = second_is_better(from_numpy(positions.leads).double(), from_numpy(positions.lead_errors).double(), config.prior)
    searched = from_numpy(positions.searched)

    def measure(epoch: int) -> None:
        report(
            evaluate(
                policy,
                validation,
                searched[validation_rows],
                better[validation_rows],
                from_numpy(positions.candidates)[validation_rows],
                config,
                epoch,
            )
        )

    measure(0)
    optimizer = torch.optim.Adam(policy.network.parameters(), lr=config.learning_rate)
    generator = torch.Generator(device="cpu").manual_seed(config.seed)
    symmetries = torch.Generator(device="cpu").manual_seed(config.seed + 1)
    for epoch in range(1, config.epochs + 1):
        policy.network.train(True)
        order = torch.randperm(len(training.actions), generator=generator)
        for start in range(0, len(order), config.batch_size):
            chunk = training.select(order[start : start + config.batch_size]).to(device).relabeled(symmetries)
            learn_batch(policy.network, optimizer, chunk, 0.0)
        measure(epoch)
    policy.network.eval()
    metadata = replace(policy.metadata, training_seeds=tuple(sorted(seeds | set(policy.metadata.training_seeds))))
    return TrainedPolicy(policy.network, metadata, policy.device)


class ExpertArguments(argparse.Namespace):
    command: str
    output: Path
    student: Path
    field: str
    rules: str
    start_seed: int
    hands: int
    samples: int
    confident: float
    anchors_per_hand: int
    workers: int
    seed: int
    checkpoint: Path
    positions: list[Path]
    prior: float
    epochs: int
    batch_size: int
    learning_rate: float
    validation_every: int
    device: DeviceName


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    gather = commands.add_parser("collect", help="label unsure student decisions by playouts")
    gather.add_argument("output", type=Path)
    gather.add_argument("--student", type=Path, required=True, help="suit-resnet-v2 checkpoint")
    gather.add_argument("--field", required=True, help="opponent preset, BotSpec JSON file, or neural strategy file")
    gather.add_argument("--rules", required=True, help="preset or Ruleset JSON file")
    gather.add_argument("--start-seed", type=int, required=True)
    gather.add_argument("--hands", type=int, required=True)
    gather.add_argument("--samples", type=int, required=True, help="playouts per candidate")
    gather.add_argument("--confident", type=float, required=True, help="favourites at least this likely are not searched")
    gather.add_argument("--anchors-per-hand", type=int, required=True, help="unsearched student decisions kept per hand")
    gather.add_argument("--workers", type=int, required=True)
    gather.add_argument("--seed", type=int, required=True)
    tune = commands.add_parser("refine", help="fine-tune a policy toward search targets")
    tune.add_argument("--checkpoint", type=Path, required=True)
    tune.add_argument("--positions", type=Path, nargs="+", required=True)
    tune.add_argument("--output", type=Path, required=True)
    tune.add_argument("--prior", type=float, required=True, help="prior standard deviation of true leads, in units")
    tune.add_argument("--epochs", type=int, required=True)
    tune.add_argument("--batch-size", type=int, required=True)
    tune.add_argument("--learning-rate", type=float, required=True)
    tune.add_argument("--validation-every", type=int, required=True, help="hands whose seed is a multiple of this validate")
    tune.add_argument("--seed", type=int, required=True)
    tune.add_argument("--device", choices=("cpu", "mps", "cuda"), required=True)
    args = parser.parse_args(namespace=ExpertArguments())
    if args.command == "collect":
        if args.hands <= 0 or args.workers <= 0:
            parser.error("--hands and --workers must be positive")
        collection = CollectionConfig(args.samples, args.confident, args.anchors_per_hand, args.seed)
        bounds = np.linspace(args.start_seed, args.start_seed + args.hands, args.workers + 1).astype(int)
        jobs = [
            CollectionJob(str(args.student), args.field, args.rules, range(a, b), collection)
            for a, b in zip(bounds[:-1], bounds[1:], strict=True)
            if b > a
        ]
        with multiprocessing.get_context("spawn").Pool(len(jobs)) as pool:
            positions = concatenate(pool.map(run_job, jobs))
        save_positions(args.output, positions)
        print(f"wrote {len(positions.seeds)} positions ({int(positions.searched.sum())} searched) to {args.output}")
    else:
        policy = load_policy(args.checkpoint, torch.device(args.device))
        positions = concatenate([load_positions(path) for path in args.positions])
        refinement = RefineConfig(
            args.prior, args.epochs, args.batch_size, args.learning_rate, args.validation_every, args.seed, args.device
        )
        refined = refine(policy, positions, refinement, lambda metrics: print(msgspec.json.encode(metrics).decode(), flush=True))
        save_policy(args.output, refined)
        print(f"saved {args.output}")


if __name__ == "__main__":
    main()
