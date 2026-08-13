// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { ICiphernodeRegistry } from "../interfaces/ICiphernodeRegistry.sol";

/// @notice Updates a committee's top-N candidates and their collateral obligations.
library RegistrySortitionLib {
    function insertCandidate(
        ICiphernodeRegistry.Committee storage committee,
        IBondingRegistry bondingRegistry,
        uint256 e3Id,
        address node,
        uint256 score
    ) external {
        address[] storage top = committee.topNodes;
        uint256 cap = committee.threshold[1];
        address displaced;

        if (top.length < cap) {
            top.push(node);
        } else {
            uint256 worstIndex;
            uint256 worstScore = committee.scoreOf[top[0]];
            for (uint256 i = 1; i < top.length; ++i) {
                uint256 candidateScore = committee.scoreOf[top[i]];
                if (candidateScore > worstScore) {
                    worstScore = candidateScore;
                    worstIndex = i;
                }
            }

            if (score >= worstScore) return;
            displaced = top[worstIndex];
            top[worstIndex] = node;
        }

        committee.scoreOf[node] = score;
        bondingRegistry.setCommitteeObligation(e3Id, node, true);
        if (displaced != address(0)) {
            bondingRegistry.setCommitteeObligation(e3Id, displaced, false);
        }
    }
}
