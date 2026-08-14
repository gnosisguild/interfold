// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import {
    Checkpoints
} from "@openzeppelin/contracts/utils/structs/Checkpoints.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";

import { IBondedCheckpoints } from "../interfaces/IBondedCheckpoints.sol";

/**
 * @title BondedCheckpoints
 * @notice Historical bonded-collateral totals for `BondingRegistry`.
 *
 * @dev Bonded FOLD is transferred to the registry, which never delegates it. Under ERC20Votes an
 * undelegated balance carries no voting power, so an operator forfeits that weight entirely —
 * while the bonded tokens still count in the token's total supply and so raise the quorum
 * denominator they cannot help meet.
 *
 * The history lives here rather than on the registry because `BondingRegistry` is within a few
 * hundred bytes of the EIP-170 limit and cannot hold it. Keeping only the write call there also
 * keeps the read surface — this contract — replaceable without touching the registry's storage.
 *
 * Timepoints are `block.timestamp`, matching `InterfoldToken`'s ERC-6372 `mode=timestamp` clock.
 * A block-number key would silently disagree with the token's own checkpoints, and an adapter
 * summing the two would report a number for two unrelated points in time.
 */
contract BondedCheckpoints is IBondedCheckpoints {
    using Checkpoints for Checkpoints.Trace208;

    /// @inheritdoc IBondedCheckpoints
    address public immutable registry;

    /// @dev Per-owner history of bonded totals.
    mapping(address bondOwner => Checkpoints.Trace208) private _history;

    /**
     * @param _registry The bonding registry allowed to write history. Immutable, so a compromised
     * owner cannot redirect who may rewrite voting weight — there is no owner.
     */
    constructor(address _registry) {
        if (_registry == address(0)) revert ZeroAddress();
        registry = _registry;
    }

    /// @inheritdoc IBondedCheckpoints
    function sync(address bondOwner, uint256 amount) external {
        if (msg.sender != registry) revert OnlyRegistry(msg.sender);

        Checkpoints.Trace208 storage history = _history[bondOwner];

        // A write that records the value already there still costs a fresh slot once the timestamp
        // has moved on, and `BondingRegistry.resyncBondedCheckpoint` is permissionless — so
        // without this anyone could grow any owner's history by a checkpoint per block, for as
        // long as they cared to pay. Skipping is exact, not an approximation: a lookup at the
        // skipped timepoint resolves to the preceding entry, which carries the same value. An
        // empty history reads as zero, so a first sync of zero is correctly a no-op too.
        if (history.latest() == amount) return;

        uint48 timepoint = clock();
        // Pushing twice at one timestamp overwrites rather than appends, so several mutations in
        // a block collapse to the final value.
        history.push(timepoint, SafeCast.toUint208(amount));

        emit BondedCheckpointed(bondOwner, timepoint, amount);
    }

    /// @inheritdoc IBondedCheckpoints
    function getPastBonded(
        address account,
        uint256 timepoint
    ) external view returns (uint256) {
        // Same rule ERC20Votes applies. Answering for an unsettled timepoint would let a caller
        // read a bond made after the snapshot it is asking about, and be counted twice: once from
        // the token balance held at the snapshot, once from the bond.
        uint48 currentClock = clock();
        if (timepoint >= currentClock) {
            revert FutureLookup(timepoint, currentClock);
        }

        return
            _history[account].upperLookupRecent(SafeCast.toUint48(timepoint));
    }

    /// @inheritdoc IBondedCheckpoints
    function bonded(address account) external view returns (uint256) {
        return _history[account].latest();
    }

    /// @inheritdoc IBondedCheckpoints
    function clock() public view returns (uint48) {
        return SafeCast.toUint48(block.timestamp);
    }

    /// @inheritdoc IBondedCheckpoints
    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() external pure returns (string memory) {
        return "mode=timestamp";
    }
}
