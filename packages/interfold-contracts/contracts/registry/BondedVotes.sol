// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IVotes } from "@openzeppelin/contracts/governance/utils/IVotes.sol";
import { IERC5805 } from "@openzeppelin/contracts/interfaces/IERC5805.sol";
import { IBondedCheckpoints } from "../interfaces/IBondedCheckpoints.sol";

/**
 * @title BondedVotes
 * @notice Voting power that counts FOLD held in a wallet and FOLD bonded as an operator.
 *
 * @dev Bonding transfers FOLD to `BondingRegistry`, which never delegates it. Under ERC20Votes an
 * undelegated balance carries no voting power, so those votes are not moved to the registry —
 * they cease to exist. An operator therefore trades governance weight for the right to run a
 * ciphernode, and the more of the supply that is bonded, the harder any vote is to pass: bonded
 * FOLD still counts in `getPastTotalSupply`, so it raises the quorum denominator while being
 * unable to help meet it.
 *
 * This contract restores that weight by reading both sources at the same timepoint. It holds no
 * state and no privileges: it is a view over the token and the registry, so it can be deployed,
 * replaced or ignored without touching either.
 *
 * Delegation is deliberately not forwarded for the bonded part. `IVotes.delegate` here would have
 * to move a position the registry owns, which this contract cannot do. Wallet-held FOLD keeps its
 * normal delegation through the token itself.
 */
contract BondedVotes is IERC5805 {
    /// @notice The FOLD token. Supplies wallet-held voting power and the total supply.
    IVotes public immutable token;

    /// @notice Records bonded totals over time for the registry holding bonded FOLD.
    IBondedCheckpoints public immutable checkpoints;

    /// @notice Thrown when a constructor argument is the zero address.
    error ZeroAddress();

    /// @notice Thrown when the token and the history do not agree on what a timepoint means.
    error ClockMismatch(uint48 tokenClock, uint48 checkpointsClock);

    /// @notice Thrown for the delegation entry points, which this view cannot honour.
    error DelegationNotSupported();

    /**
     * @param _token The FOLD token.
     * @param _checkpoints The bonded-history contract.
     */
    constructor(IVotes _token, IBondedCheckpoints _checkpoints) {
        if (address(_token) == address(0)) revert ZeroAddress();
        if (address(_checkpoints) == address(0)) revert ZeroAddress();

        // Checked once at deployment rather than on every read. Summing a timestamp-keyed history
        // with a block-numbered one would return a number for two unrelated points in time, and
        // nothing downstream could detect it.
        //
        // Compared by value rather than by `CLOCK_MODE()` string: two clocks that agree on the
        // current timepoint agree on every timepoint, and a block height cannot coincide with a
        // unix timestamp on any live chain. It also catches a clock that drifted for a reason no
        // mode string would describe.
        uint48 tokenClock = IERC5805(address(_token)).clock();
        uint48 registryClock = _checkpoints.clock();
        if (tokenClock != registryClock) {
            revert ClockMismatch(tokenClock, registryClock);
        }

        token = _token;
        checkpoints = _checkpoints;
    }

    /// @notice Get the current timepoint, in ERC-6372 clock units.
    /// @dev Delegated to the token so this adapter can never disagree with it about what a
    /// timepoint means. The constructor already established that the registry agrees too.
    /// @return Current timepoint.
    function clock() public view returns (uint48) {
        return IERC5805(address(token)).clock();
    }

    /// @notice Get the ERC-6372 description of this adapter's clock.
    /// @return Machine-readable clock mode, as reported by the token.
    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() external view returns (string memory) {
        return IERC5805(address(token)).CLOCK_MODE();
    }

    /// @inheritdoc IVotes
    function getPastVotes(
        address account,
        uint256 timepoint
    ) external view returns (uint256) {
        return
            token.getPastVotes(account, timepoint) +
            checkpoints.getPastBonded(account, timepoint);
    }

    /// @inheritdoc IVotes
    /// @dev Passed through unchanged. Bonded FOLD is already counted in the token's supply — it
    /// was transferred, not burned — so adding the bonded total again would double it and inflate
    /// every quorum denominator. Leaving it alone is what makes bonded weight *fix* the quorum
    /// distortion rather than compound it.
    function getPastTotalSupply(
        uint256 timepoint
    ) external view returns (uint256) {
        return token.getPastTotalSupply(timepoint);
    }

    /// @inheritdoc IVotes
    /// @dev Both halves read the present. Pairing a current wallet balance with
    /// `getPastBonded(account, clock() - 1)` would sum two different instants: a claim or a slash
    /// in this block would leave the bonded half stale and high, so the total could exceed what
    /// the owner holds — and, summed across owners, exceed total supply.
    function getVotes(address account) external view returns (uint256) {
        return token.getVotes(account) + checkpoints.bonded(account);
    }

    /// @inheritdoc IVotes
    /// @dev The token's delegate. Bonded weight is not delegatable, so it always sits with the
    /// bond owner regardless of what this returns.
    function delegates(address account) external view returns (address) {
        return token.delegates(account);
    }

    /// @inheritdoc IVotes
    /// @dev Not supported. Delegate on the token directly — this contract owns no position and
    /// cannot move the registry's. Reverting rather than silently doing nothing, so a caller
    /// cannot believe it delegated bonded weight.
    function delegate(address) external pure {
        revert DelegationNotSupported();
    }

    /// @inheritdoc IVotes
    /// @dev Not supported, for the same reason as {delegate}.
    function delegateBySig(
        address,
        uint256,
        uint256,
        uint8,
        bytes32,
        bytes32
    ) external pure {
        revert DelegationNotSupported();
    }
}
