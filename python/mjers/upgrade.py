"""Let a version 1 suit-resnet-v2 policy read current observations without changing its play.

The stem gains zero-weight input columns for the planes encoding version 2
appends, so the upgraded network computes the same logits and value until
training (for example `mjers.reinforce`) learns to use them. The checkpoint
records the current encoding version, which is what its readers check, and
keeps its training and reinforcement seed history.
"""

import argparse
from dataclasses import replace
from pathlib import Path

import torch

from mjers._native import CONTEXT_SIZE, ENCODING_VERSION, TILE_PLANES
from mjers.model import ResidualPolicyNet, TrainedPolicy, load_policy, save_policy


def upgrade(policy: TrainedPolicy) -> TrainedPolicy:
    network = policy.network
    if not isinstance(network, ResidualPolicyNet):
        raise ValueError("only suit-resnet-v2 policies can be upgraded")
    if network.planes == TILE_PLANES:
        raise ValueError("the policy already reads current observations")
    upgraded = ResidualPolicyNet(network.hidden_width, len(network.blocks), TILE_PLANES).to(policy.device)
    state = network.state_dict()
    stem = state["stem.weight"]
    # Stem inputs are the tile planes followed by the broadcast context.
    weight = stem.new_zeros(stem.shape[0], TILE_PLANES + CONTEXT_SIZE, stem.shape[2])
    weight[:, : network.planes] = stem[:, : network.planes]
    weight[:, TILE_PLANES:] = stem[:, network.planes :]
    upgraded.load_state_dict({**state, "stem.weight": weight}, strict=True)
    upgraded.eval()
    dataset = replace(policy.metadata.dataset, encoding_version=ENCODING_VERSION)
    return TrainedPolicy(upgraded, replace(policy.metadata, dataset=dataset), policy.device)


class UpgradeArguments(argparse.Namespace):
    checkpoint: Path
    output: Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", type=Path, required=True, help="version 1 suit-resnet-v2 policy")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(namespace=UpgradeArguments())
    save_policy(args.output, upgrade(load_policy(args.checkpoint, torch.device("cpu"))))
    print(f"saved {args.output} reading encoding {ENCODING_VERSION}")


if __name__ == "__main__":
    main()
