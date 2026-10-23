"""Add residual blocks to a suit-resnet-v2 policy without changing its play.

New blocks are appended after the existing ones. Each starts as the identity
(its second convolutions are zero), so the deeper network computes exactly the
same logits and value, and reinforcement can then use the extra capacity. The
checkpoint keeps its training and reinforcement seed history.
"""

import argparse
from dataclasses import replace
from pathlib import Path

import torch

from mjers.model import ResidualPolicyNet, TrainedPolicy, load_policy, save_policy


def deepen(policy: TrainedPolicy, residual_blocks: int) -> TrainedPolicy:
    network = policy.network
    if not isinstance(network, ResidualPolicyNet):
        raise ValueError("only suit-resnet-v2 policies have residual blocks")
    if residual_blocks <= len(network.blocks):
        raise ValueError("the deeper network needs more blocks than the original")
    deeper = ResidualPolicyNet(network.hidden_width, residual_blocks).to(policy.device)
    missing, unexpected = deeper.load_state_dict(network.state_dict(), strict=False)
    new_block = tuple(f"blocks.{index}." for index in range(len(network.blocks), residual_blocks))
    if unexpected or not all(name.startswith(new_block) for name in missing):
        raise ValueError("the original network's weights do not fit the deeper network")
    deeper.eval()
    return TrainedPolicy(deeper, replace(policy.metadata, residual_blocks=residual_blocks), policy.device)


class DeepenArguments(argparse.Namespace):
    checkpoint: Path
    output: Path
    residual_blocks: int


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", type=Path, required=True, help="suit-resnet-v2 policy")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--residual-blocks", type=int, required=True, help="total blocks in the deeper network")
    args = parser.parse_args(namespace=DeepenArguments())
    deeper = deepen(load_policy(args.checkpoint, torch.device("cpu")), args.residual_blocks)
    save_policy(args.output, deeper)
    print(f"saved {args.output} with {args.residual_blocks} residual blocks")


if __name__ == "__main__":
    main()
