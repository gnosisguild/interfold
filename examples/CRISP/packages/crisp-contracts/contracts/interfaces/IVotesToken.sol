// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

/// @notice The subset of an ERC20Votes token that `CensusMode.ONCHAIN` reads.
/// @dev Required, not optional. A round that names a token which cannot answer this is rejected
/// at request time, because every input would otherwise revert here after the fee is paid.
interface IVotesToken {
  /// @notice The voting power of an account at a past timepoint.
  /// @param account The account to read.
  /// @param timepoint The timepoint, in the ERC-6372 clock units of the token.
  /// @return The voting power at that timepoint.
  function getPastVotes(address account, uint256 timepoint) external view returns (uint256);
}
