// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { IInterfold } from "../interfaces/IInterfold.sol";
import { ISlashingManager } from "../interfaces/ISlashingManager.sol";

contract MockSlashingInterfold {
    mapping(uint256 e3Id => uint256 deadline) private _lifecycleDeadlines;

    function snapshotDependencies(
        ISlashingManager manager,
        uint256 e3Id,
        uint256 lifecycleDeadline
    ) external {
        _lifecycleDeadlines[e3Id] = lifecycleDeadline;
        manager.snapshotE3Dependencies(e3Id);
    }

    function getE3LifecycleDeadline(
        uint256 e3Id
    ) external view returns (uint256) {
        return _lifecycleDeadlines[e3Id];
    }

    function getE3Stage(uint256) external pure returns (IInterfold.E3Stage) {
        return IInterfold.E3Stage.Failed;
    }
}
