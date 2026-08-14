// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { ICiphernodeRegistry } from "../interfaces/ICiphernodeRegistry.sol";
import { IArbSys } from "../interfaces/external/IArbSys.sol";

/// @notice Resolves entropy and updates candidate rankings for registry sortition.
library RegistrySortitionLib {
    uint256 private constant ARBITRUM_ONE_CHAIN_ID = 42161;
    uint256 private constant ARBITRUM_NOVA_CHAIN_ID = 42170;
    uint256 private constant ARBITRUM_SEPOLIA_CHAIN_ID = 421614;

    IArbSys private constant ARBSYS = IArbSys(address(100));
    address private constant BLOCKHASH_HISTORY =
        0x0000F90827F1C53a10cb7A02335B175320002935;

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

    /// @notice Returns the chain block number that identifies RPC block hashes.
    function currentBlockNumber(
        uint256 chainId
    ) external view returns (uint256) {
        if (_usesArbitrumBlockNumbers(chainId)) {
            return ARBSYS.arbBlockNumber();
        }
        return block.number;
    }

    /// @notice Returns the committed chain block hash when it is available.
    function entropyBlockHash(
        uint256 chainId,
        uint256 entropyBlock
    ) external view returns (bool ready, bytes32 blockHash) {
        uint256 currentBlock = block.number;
        bool usesArbitrumBlocks = _usesArbitrumBlockNumbers(chainId);
        if (usesArbitrumBlocks) currentBlock = ARBSYS.arbBlockNumber();
        if (entropyBlock == 0 || currentBlock <= entropyBlock) {
            return (false, bytes32(0));
        }

        if (!usesArbitrumBlocks) {
            blockHash = blockhash(entropyBlock);
            if (blockHash != bytes32(0)) return (true, blockHash);
        }

        (bool success, bytes memory result) = BLOCKHASH_HISTORY.staticcall(
            abi.encode(entropyBlock)
        );
        if (success && result.length == 32) {
            blockHash = abi.decode(result, (bytes32));
        }
        ready = blockHash != bytes32(0);
    }

    function _usesArbitrumBlockNumbers(
        uint256 chainId
    ) private pure returns (bool) {
        return
            chainId == ARBITRUM_ONE_CHAIN_ID ||
            chainId == ARBITRUM_NOVA_CHAIN_ID ||
            chainId == ARBITRUM_SEPOLIA_CHAIN_ID;
    }
}
