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
    ) VRFCoordinatorV2_5Mock(baseFee, gasPrice, weiPerUnitLink) {}
}
