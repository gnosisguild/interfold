// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/// @notice Minimal interface for the Arbitrum system precompile.
interface IArbSys {
    /// @notice Returns the current Arbitrum L2 block number.
    function arbBlockNumber() external view returns (uint256);
}
