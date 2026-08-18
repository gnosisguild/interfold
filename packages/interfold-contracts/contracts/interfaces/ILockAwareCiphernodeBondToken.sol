// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

interface ILockAwareCiphernodeBondToken {
    function lockedBalanceOf(address account) external view returns (uint256);

    /// @dev The same schedule as {lockedBalanceOf}, evaluated against an arbitrary timestamp
    /// rather than the present. `BondedVotes` reads it so a governance snapshot counts the FOLD a
    /// holder had encumbered at the snapshot, not what it has encumbered now.
    function lockedBalanceAt(
        address account,
        uint64 timestamp
    ) external view returns (uint256);
}
