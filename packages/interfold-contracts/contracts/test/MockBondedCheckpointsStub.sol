// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/**
 * @title MockBondedCheckpointsStub
 * @notice A bonded history and its registry in one contract, for tests that only need
 * `BondedVotes` to get PAST the history binding rather than to read any bond.
 *
 * @dev `BondedVotes` binds token, registry and history in its constructor: the history must share
 * the token's clock, and the registry that wrote it must bond the same token. Satisfying that with
 * the real contracts means a full system deployment plus a bonding-asset rotation, which is more
 * than a test of a LATER constructor check needs. This answers `registry()` with itself and
 * `getCiphernodeBondToken()` with whatever token the test is binding, so the checks pass and
 * construction reaches the one being measured.
 *
 * Records no history: `bonded` and `getPastBonded` are zero. A test that needs bonded weight must
 * use the real `BondedCheckpoints`.
 */
contract MockBondedCheckpointsStub {
    address public immutable ciphernodeBondToken;

    constructor(address _ciphernodeBondToken) {
        ciphernodeBondToken = _ciphernodeBondToken;
    }

    /// @dev Itself, so one deployment serves as both halves of the binding.
    function registry() external view returns (address) {
        return address(this);
    }

    function getCiphernodeBondToken() external view returns (address) {
        return ciphernodeBondToken;
    }

    /// @dev `mode=timestamp`, matching `InterfoldToken` and the real history.
    function clock() external view returns (uint48) {
        return uint48(block.timestamp);
    }

    function bonded(address) external pure returns (uint256) {
        return 0;
    }

    function getPastBonded(address, uint256) external pure returns (uint256) {
        return 0;
    }
}
