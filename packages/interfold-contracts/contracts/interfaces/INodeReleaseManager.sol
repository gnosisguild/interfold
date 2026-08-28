// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { INodeReleaseRegistry } from "./INodeReleaseRegistry.sol";
import { IBondingRegistry } from "./IBondingRegistry.sol";
import { ICiphernodeRegistry } from "./ICiphernodeRegistry.sol";

/// @notice Exposes the release controller selected by the Interfold dependency graph.
interface INodeReleaseManager {
    event NodeReleaseRegistrySet(address indexed nodeReleaseRegistry);

    function nodeReleaseRegistry() external view returns (INodeReleaseRegistry);

    function bondingRegistry() external view returns (IBondingRegistry);

    function ciphernodeRegistry() external view returns (ICiphernodeRegistry);

    function setNodeReleaseRegistry(
        INodeReleaseRegistry newNodeReleaseRegistry
    ) external;
}
