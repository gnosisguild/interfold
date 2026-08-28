// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import {
    VRFCoordinatorV2_5Mock
} from "@chainlink/contracts/src/v0.8/vrf/mocks/VRFCoordinatorV2_5Mock.sol";

/// @notice Makes Chainlink's coordinator mock available to the Hardhat test build.
contract ChainlinkVrfCoordinatorV2_5Mock is VRFCoordinatorV2_5Mock {
    constructor(
        uint96 baseFee,
        uint96 gasPrice,
        int256 weiPerUnitLink
    ) VRFCoordinatorV2_5Mock(baseFee, gasPrice, weiPerUnitLink) {
        s_config.minimumRequestConfirmations = 1;
        s_config.maxGasLimit = 2_500_000;
    }

    /// @notice Mirrors the production coordinator's public gas-lane lookup.
    function s_provingKeys(
        bytes32 keyHash
    ) external pure returns (bool exists, uint64 maxGas) {
        return (keyHash != bytes32(0), type(uint64).max);
    }
}
