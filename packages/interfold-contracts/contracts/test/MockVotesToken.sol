// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { ERC20 } from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import { EIP712 } from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {
    ERC20Votes
} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Votes.sol";

/**
 * @title MockVotesToken
 * @notice Stand-in for FOLD where a test needs a votes token but not the real one.
 *
 * @dev `InterfoldToken` cannot be constructed without a deployed bonding registry and a future CCA
 * window, which is more setup than a test that only needs the token's voting surface. This carries
 * the part `BondedVotes` reads: ERC20Votes power and an ERC-6372 `mode=timestamp` clock, matching
 * `InterfoldToken`. A block-numbered clock here would make `BondedVotes` reject it at construction.
 */
contract MockVotesToken is ERC20Votes {
    constructor() ERC20("Mock Votes", "MVOTE") EIP712("Mock Votes", "1") {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function clock() public view override returns (uint48) {
        return uint48(block.timestamp);
    }

    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() public pure override returns (string memory) {
        return "mode=timestamp";
    }
}
