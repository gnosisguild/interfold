// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { ExitQueueLib } from "../lib/ExitQueueLib.sol";

contract ExitQueueHarness {
    using ExitQueueLib for ExitQueueLib.ExitQueueState;

    ExitQueueLib.ExitQueueState private _state;

    function queue(
        address operator,
        uint64 delay,
        uint256 ticketAmount,
        uint256 licenseAmount
    ) external {
        _state.queueAssetsForExit(operator, delay, ticketAmount, licenseAmount);
    }

    function slash(
        address operator,
        uint256 ticketAmount,
        uint256 licenseAmount
    ) external returns (uint256 ticketsSlashed, uint256 licensesSlashed) {
        return
            _state.slashPendingAssets(
                operator,
                ticketAmount,
                licenseAmount,
                true
            );
    }

    function claim(
        address operator,
        uint256 ticketAmount,
        uint256 licenseAmount
    ) external returns (uint256 ticketsClaimed, uint256 licensesClaimed) {
        return _state.claimAssets(operator, ticketAmount, licenseAmount);
    }

    function queueSlashQueue(
        address operator,
        uint64 delay,
        uint256 firstTicketAmount,
        uint256 secondTicketAmount
    ) external {
        _state.queueTicketsForExit(operator, delay, firstTicketAmount);
        _state.slashPendingAssets(operator, firstTicketAmount, 0, true);
        _state.queueTicketsForExit(operator, delay, secondTicketAmount);
    }

    function queueTicketThenLicense(
        address operator,
        uint64 delay,
        uint256 ticketAmount,
        uint256 licenseAmount
    ) external {
        _state.queueTicketsForExit(operator, delay, ticketAmount);
        _state.queueLicensesForExit(operator, delay, licenseAmount);
    }

    function liveTrancheCount(
        address operator
    ) external view returns (uint256) {
        return _state.liveTrancheCount[operator];
    }

    function tranche(
        address operator,
        uint256 index
    ) external view returns (ExitQueueLib.ExitTranche memory) {
        return _state.operatorQueues[operator][index];
    }

    function queueLength(address operator) external view returns (uint256) {
        return _state.operatorQueues[operator].length;
    }
}
