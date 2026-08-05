// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { CommitteeHashLib } from "../lib/CommitteeHashLib.sol";

contract CommitteeHashHarness {
    function hash(address[] memory nodes) external pure returns (bytes32) {
        return CommitteeHashLib.hash(nodes);
    }
}
