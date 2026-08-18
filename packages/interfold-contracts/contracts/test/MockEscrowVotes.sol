// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

/**
 * @title MockVotingEscrow
 * @notice The slice of a voting escrow `BondedVotes` reads: what it custodies, and how much of
 * that it attributes to an account.
 *
 * @dev Stands in for the real escrow, whose voting-power curve is flat — locked FOLD produces
 * exactly its own amount in voting power, with no decay and no boost. That is what makes escrow
 * power addable to bonded FOLD at all, so the mock reproduces it by storing amounts directly.
 */
contract MockVotingEscrow {
    address public token;
    mapping(address account => uint256 amount) private _locked;

    constructor(address _token) {
        token = _token;
    }

    function setLocked(address account, uint256 amount) external {
        _locked[account] = amount;
    }

    /// @dev Delegation-blind, matching the real escrow: it answers what the account itself locked.
    function votingPowerForAccount(
        address account
    ) external view returns (uint256) {
        return _locked[account];
    }

    /// @dev Lets a test point the escrow at a different token without redeploying the adapter,
    /// to exercise the constructor's binding check.
    function setToken(address _token) external {
        token = _token;
    }
}

/**
 * @title MockEscrowVotesAdapter
 * @notice An IVotes view over {MockVotingEscrow}, mirroring the real `EscrowIVotesAdapter`.
 *
 * @dev Deliberately NOT an ERC20: the real adapter has no `decimals`, `name`, `symbol` or
 * `totalSupply`, and its `balanceOf` counts lock NFTs rather than tokens. `BondedVotes` must
 * therefore never read metadata or supply through it — these tests exist to hold that line.
 */
contract MockEscrowVotesAdapter {
    address public escrow;
    uint48 private _clockOverride;
    bool private _clockOverridden;

    mapping(address account => uint256 power) private _votes;
    mapping(address account => address delegatee) private _delegates;

    constructor(address _escrow) {
        escrow = _escrow;
    }

    function setVotes(address account, uint256 power) external {
        _votes[account] = power;
    }

    function setDelegate(address account, address delegatee) external {
        _delegates[account] = delegatee;
    }

    /// @dev Forces a clock the token cannot agree with, to exercise the mismatch guard.
    function setClock(uint48 value) external {
        _clockOverride = value;
        _clockOverridden = true;
    }

    function clock() external view returns (uint48) {
        if (_clockOverridden) return _clockOverride;
        return uint48(block.timestamp);
    }

    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() external pure returns (string memory) {
        return "mode=timestamp";
    }

    function getVotes(address account) external view returns (uint256) {
        return _votes[account];
    }

    /// @dev Flat over time, like the escrow's curve: history is not what these tests measure.
    function getPastVotes(
        address account,
        uint256
    ) external view returns (uint256) {
        return _votes[account];
    }

    function getPastTotalSupply(uint256) external pure returns (uint256) {
        return 0;
    }

    function delegates(address account) external view returns (address) {
        return _delegates[account];
    }

    function delegate(address) external pure {
        revert("MockEscrowVotesAdapter: not supported");
    }

    function delegateBySig(
        address,
        uint256,
        uint256,
        uint8,
        bytes32,
        bytes32
    ) external pure {
        revert("MockEscrowVotesAdapter: not supported");
    }
}
