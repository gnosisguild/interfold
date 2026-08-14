// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

/// @notice The ERC-6372 clock of a token, used to record the snapshot of a round.
/// @dev Kept separate from `IVotesToken` because it is the optional half of the token surface.
/// A token that predates ERC-6372 still answers `getPastVotes`, so `CRISPProgram` falls back to
/// block numbers when this call reverts rather than refusing the round.
interface IERC6372Clock {
  /// @notice The current timepoint, in the clock units of the token.
  /// @return The timepoint.
  function clock() external view returns (uint48);
}
