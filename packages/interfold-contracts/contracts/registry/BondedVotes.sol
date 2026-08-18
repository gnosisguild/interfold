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

/// @dev The slice of a voting-escrow IVotes adapter this contract needs to bind it to a token.
/// Only consulted when the votes source is not the token itself.
interface IEscrowVotesSource {
    function escrow() external view returns (address);
}

/// @dev The slice of a voting escrow this contract needs: what it custodies, and how much of it
/// it attributes to an account irrespective of delegation.
interface IVotingEscrow {
    function token() external view returns (address);

    function votingPowerForAccount(
        address account
    ) external view returns (uint256);
}

/**
 * @title BondedVotes
 * @notice Voting power that counts FOLD bonded as an operator alongside a primary vote source.
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
 * TWO TOKEN REFERENCES, ON PURPOSE. `token` is FOLD: it supplies the metadata and, critically,
 * the quorum DENOMINATOR. `votesSource` supplies the per-account NUMERATOR. They are separate
 * because the two questions have different answers under a lock-to-vote model:
 *
 *   votesSource == token          wallet-held FOLD votes, via the token's own ERC20Votes
 *                                 delegation. The original behaviour.
 *   votesSource == an escrow      only FOLD locked in the escrow votes. Idle wallet FOLD carries
 *                                 no weight, so holders must lock to participate — while
 *                                 operators keep their weight by bonding instead.
 *
 * Keeping the denominator on FOLD in both cases is what makes the ratio sound. Every unit the
 * numerator can produce — locked or bonded — is a FOLD that is still inside FOLD's total supply,
 * because locking and bonding both TRANSFER the token rather than burn it. So summed votes can
 * never exceed the supply they are measured against. Reading the denominator off the escrow
 * instead would omit the bonded half entirely and let participation exceed 100%.
 *
 * Locked and bonded FOLD cannot overlap: locking custodies the token in the escrow and bonding
 * custodies it in the registry, so the same token can never be counted twice.
 *
 * Delegation is deliberately not forwarded for the bonded part. `IVotes.delegate` here would have
 * to move a position the registry owns, which this contract cannot do. The primary source keeps
 * its own delegation, whatever that means for it.
 */
contract BondedVotes is IERC5805 {
    /// @notice The FOLD token. Supplies the metadata and the quorum denominator.
    IVotes public immutable token;

    /// @notice Where per-account voting power is read from: the token itself, or an escrow
    /// adapter when only locked FOLD may vote.
    IVotes public immutable votesSource;

    /// @notice The escrow behind `votesSource`, or zero when the votes source is the token.
    /// @dev Held so {balanceOf} can attribute locked FOLD without going through the adapter's
    /// own `balanceOf`, which counts lock NFTs rather than tokens.
    address public immutable escrow;

    /// @notice Records bonded totals over time for the registry holding bonded FOLD.
    IBondedCheckpoints public immutable checkpoints;

    /// @notice The registry that custodies the bonded FOLD and writes the history.
    address public immutable registry;

    /// @notice Thrown when a constructor argument is the zero address.
    error ZeroAddress();

    /// @notice Thrown when the token and the history do not agree on what a timepoint means.
    error ClockMismatch(uint48 tokenClock, uint48 checkpointsClock);

    /// @notice Thrown when the votes source measures a token other than the one being counted.
    error VotesSourceMismatch(address escrowToken, address votingToken);

    /// @notice Thrown when the history records bonds of a token other than the one read for votes.
    error TokenMismatch(address ciphernodeBondToken, address votingToken);

    /// @notice Thrown for the delegation entry points, which this view cannot honour.
    error DelegationNotSupported();

    /**
     * @param _token The FOLD token: metadata, total supply, and the quorum denominator.
     * @param _votesSource Where per-account voting power is read. Pass `_token` itself to count
     * wallet-held FOLD, or an escrow IVotes adapter to count only locked FOLD.
     * @param _checkpoints The bonded-history contract.
     */
    constructor(
        IVotes _token,
        IVotes _votesSource,
        IBondedCheckpoints _checkpoints
    ) {
        if (address(_token) == address(0)) revert ZeroAddress();
        if (address(_votesSource) == address(0)) revert ZeroAddress();
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
        address ciphernodeBondToken = IBondingRegistry(boundRegistry)
            .getCiphernodeBondToken();
        if (ciphernodeBondToken != address(_token)) {
            revert TokenMismatch(ciphernodeBondToken, address(_token));
        }

        token = _token;
        votesSource = _votesSource;
        escrow = _bindVotesSource(_token, _votesSource, tokenClock);
        checkpoints = _checkpoints;
        registry = boundRegistry;
    }

    /// @dev Checks a votes source may be summed with this token's history, and resolves the
    /// escrow behind it.
    ///
    /// Two conditions, for the same reason `TokenMismatch` exists on the other half of the
    /// numerator. The clock, because a block-numbered source summed with a timestamp-keyed
    /// history answers for two unrelated instants and nothing downstream could detect it. The
    /// custodied token, because an escrow over something else would mint voting power this
    /// token's supply does not back — the denominator would be FOLD while the numerator counted
    /// something else, and participation could exceed 100% with nothing able to notice.
    ///
    /// A votes source that IS the token is trivially about itself, needs no escrow, and returns
    /// zero here so {balanceOf} never consults one.
    /// @return The escrow behind the votes source, or zero when it is the token itself.
    function _bindVotesSource(
        IVotes _token,
        IVotes _votesSource,
        uint48 tokenClock
    ) private view returns (address) {
        if (address(_votesSource) == address(_token)) return address(0);

        uint48 votesClock = IERC5805(address(_votesSource)).clock();
        if (tokenClock != votesClock) {
            revert ClockMismatch(tokenClock, votesClock);
        }

        address boundEscrow = IEscrowVotesSource(address(_votesSource))
            .escrow();
        address escrowToken = IVotingEscrow(boundEscrow).token();
        if (escrowToken != address(_token)) {
            revert VotesSourceMismatch(escrowToken, address(_token));
        }

        return boundEscrow;
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
    /// @dev The numerator: whatever the primary source attributes to the account, plus its bonded
    /// FOLD. Both halves are FOLD-denominated and cannot overlap, since locking and bonding
    /// custody the token in different places.
    function getPastVotes(
        address account,
        uint256 timepoint
    ) external view returns (uint256) {
        return
            votesSource.getPastVotes(account, timepoint) +
            checkpoints.getPastBonded(account, timepoint);
    }

    /// @inheritdoc IVotes
    /// @dev The denominator, and always the TOKEN's supply — never the votes source's. Bonded and
    /// locked FOLD are both already counted here: each was transferred, not burned. Adding either
    /// total again would double it and inflate every quorum denominator, and reading the escrow's
    /// supply instead would omit the bonded half and let participation exceed 100%. Leaving it
    /// alone is what makes the ratio sound in both configurations.
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
        return votesSource.getVotes(account) + checkpoints.bonded(account);
    }

    /// @inheritdoc IVotes
    /// @dev The primary source's delegate. Bonded weight is not delegatable, so it always sits
    /// with the bond owner regardless of what this returns.
    function delegates(address account) external view returns (address) {
        return votesSource.delegates(account);
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
                .totalCiphernodeBondLiability();
            // Saturating: an accounting drift must not make the balance unreadable.
            held = held > custodied ? held - custodied : 0;
        }

        // Locked FOLD is custodied by the escrow for the same reason bonded FOLD is custodied by
        // the registry, so it is attributed to the locker here rather than left sitting with the
        // contract holding it. Read per-account and delegation-blind, matching what this function
        // means; the adapter's own `balanceOf` is deliberately not used, because it counts lock
        // NFTs rather than tokens.
        if (escrow != address(0)) {
            held += IVotingEscrow(escrow).votingPowerForAccount(account);
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
