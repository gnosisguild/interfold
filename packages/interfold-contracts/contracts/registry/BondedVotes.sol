// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IVotes } from "@openzeppelin/contracts/governance/utils/IVotes.sol";
import { IERC5805 } from "@openzeppelin/contracts/interfaces/IERC5805.sol";
import {
    IERC20Metadata
} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import { IBondedCheckpoints } from "../interfaces/IBondedCheckpoints.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";

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

    /// @notice The registry that custodies the bonded FOLD and writes the history.
    address public immutable registry;

    /// @notice Thrown when a constructor argument is the zero address.
    error ZeroAddress();

    /// @notice Thrown when the token and the history do not agree on what a timepoint means.
    error ClockMismatch(uint48 tokenClock, uint48 checkpointsClock);

    /// @notice Thrown when the history records bonds of a token other than the one read for votes.
    error TokenMismatch(address licenseToken, address votingToken);

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

        // The clock check proves the history speaks the same units as the token. It does not prove
        // the history is *about* this token. A checkpoint contract written by a registry that
        // custodies something else would add unbacked weight to this token's votes, and no reader
        // downstream could tell — summed voting power would simply exceed total supply. Binding
        // token, registry and history here is what makes that unrepresentable.
        //
        // A registry with no code fails this too: the call returns nothing and decoding reverts.
        address boundRegistry = _checkpoints.registry();
        address licenseToken = IBondingRegistry(boundRegistry)
            .getLicenseToken();
        if (licenseToken != address(_token)) {
            revert TokenMismatch(licenseToken, address(_token));
        }

        token = _token;
        checkpoints = _checkpoints;
        registry = boundRegistry;
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

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                  ERC-20 read surface                   //
    //                                                        //
    ////////////////////////////////////////////////////////////
    //
    // Read-only on purpose. `transfer`, `transferFrom`, `approve` and `allowance` are deliberately
    // absent: this contract owns no position and can move nothing, so a caller that tries to spend
    // through it reverts on a missing selector instead of believing a transfer happened.
    //
    // The read half exists because Aragon's plugin setups gate installation on a `balanceOf` probe
    // — `TokenVotingSetup._isERC20` staticcalls `balanceOf(address)` and rejects the token unless
    // it returns 32 bytes — and because the governance app reads the metadata to render amounts.

    /// @notice Get the FOLD attributable to an account: held in its wallet plus bonded under it.
    /// @dev Not a spendable balance, and nothing here can move it. Unlike {getVotes} this ignores
    /// delegation, so it answers "how much FOLD is this account's" rather than "how much can it
    /// vote with". A holder that never delegated reads a balance above its votes, which is the
    /// intended signal.
    ///
    /// The registry is netted down by what it merely custodies. Bonding moves FOLD into the
    /// registry while this contract attributes it to the bond owner, so counting it at both
    /// addresses would place the same tokens twice and push the summed balances above total
    /// supply — which is exactly what any holder-percentage view divides by. What remains for the
    /// registry is genuine surplus it holds on its own account.
    /// @param account The account to read.
    /// @return Attributable wallet balance plus bonded total.
    function balanceOf(address account) external view returns (uint256) {
        uint256 held = IERC20Metadata(address(token)).balanceOf(account);

        if (account == registry) {
            uint256 custodied = IBondingRegistry(registry)
                .totalLicenseLiability();
            // Saturating: an accounting drift must not make the balance unreadable.
            held = held > custodied ? held - custodied : 0;
        }

        return held + checkpoints.bonded(account);
    }

    /// @notice Get the token's total supply.
    /// @dev Passed through for the same reason as {getPastTotalSupply}: bonded FOLD was
    /// transferred, not burned, so it is already counted and must not be added again.
    /// @return The token's total supply.
    function totalSupply() external view returns (uint256) {
        return IERC20Metadata(address(token)).totalSupply();
    }

    /// @notice Get the token's decimals.
    /// @return Decimals, as reported by the token.
    function decimals() external view returns (uint8) {
        return IERC20Metadata(address(token)).decimals();
    }

    /// @notice Get the token's name.
    /// @return Name, as reported by the token.
    function name() external view returns (string memory) {
        return IERC20Metadata(address(token)).name();
    }

    /// @notice Get the token's symbol.
    /// @return Symbol, as reported by the token.
    function symbol() external view returns (string memory) {
        return IERC20Metadata(address(token)).symbol();
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
