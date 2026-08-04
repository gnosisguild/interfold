// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { ICiphernodeRegistry } from "./ICiphernodeRegistry.sol";
import { InterfoldTicketToken } from "../token/InterfoldTicketToken.sol";

/**
 * @title IBondingRegistry
 * @notice Interface for the main bonding registry that holds operator balance and license bonds
 */
interface IBondingRegistry {
    // ======================
    // Custom Errors
    // ======================

    // General
    error ZeroAddress();
    error ZeroAmount();
    error CiphernodeBanned();
    error Unauthorized();
    error InsufficientBalance();
    error NotLicensed();
    error AlreadyRegistered();
    error NotRegistered();
    error ExitInProgress();
    error ExitNotReady();
    error InvalidAmount();

    /// @notice A bonding asset must be a deployed contract (except the one-time
    ///         zero license-token placeholder used during circular deployment).
    error InvalidBondingAsset(address asset);

    /// @notice The configured license token does not provide a valid locked balance.
    error IncompatibleLicenseToken(address token);

    /// @notice A bonding asset cannot rotate while balances remain denominated in it.
    error OutstandingAssetLiabilities(address asset, uint256 amount);

    error InvalidConfiguration();
    error NoPendingDeregistration();
    error OnlyRewardDistributor();
    error ArrayLengthMismatch();
    /// @notice Thrown when an operator attempts to withdraw collateral while any
    ///         financial slash proposal against them remains unresolved.
    error OperatorUnderSlash();

    /// @notice Treasury withdrawal or generic routing attempted to consume funds
    ///         reserved for a pending E3 slash route.
    error ReservedSlashedFunds();

    /// @notice Thrown when {setExitDelay} input is outside the permitted range.
    error ExitDelayOutOfBounds(uint64 exitDelay);

    /// @notice Thrown when {setRewardDistributor} would exceed
    ///         {MAX_AUTHORIZED_DISTRIBUTORS}.
    error MaxAuthorizedDistributors();

    /// @notice Thrown when {renounceOwnership} is called.
    error RenounceOwnershipDisabled();

    /// @notice Thrown when a caller attempts to manage another operator's collateral.
    error NotBondOwner(address caller, address operator);

    /// @notice Thrown when an operator attempts to replace an already-authorized bond owner.
    error BondOwnerAlreadySet(address operator, address bondOwner);

    /// @notice Moving a license position would leave the previous owner with
    ///         less wallet-plus-bonded FOLD than its current locked balance.
    error BondOwnerTransferViolatesLock(
        address bondOwner,
        uint256 lockedBalance,
        uint256 controlledBalance
    );

    // ======================
    // Events (Protocol-Named)
    // ======================

    /**
     * @notice Emitted when operator's ticket balance changes
     * @param operator Address of the operator
     * @param delta Change in balance (positive for increase, negative for decrease)
     * @param newBalance New total balance
     * @param reason Reason for the change (e.g., "DEPOSIT", "WITHDRAW", slash reason)
     */
    event TicketBalanceUpdated(
        address indexed operator,
        int256 delta,
        uint256 newBalance,
        bytes32 indexed reason
    );

    /**
     * @notice Emitted when operator's license bond changes
     * @param operator Address of the operator
     * @param delta Change in bond (positive for increase, negative for decrease)
     * @param newBond New total license bond
     * @param reason Reason for the change (e.g., "BOND", "UNBOND", slash reason)
     */
    event LicenseBondUpdated(
        address indexed operator,
        int256 delta,
        uint256 newBond,
        bytes32 indexed reason
    );

    /**
     * @notice Emitted when operator requests deregistration from the protocol
     * @param operator Address of the operator
     * @param unlockAt Timestamp when deregistration can be finalized
     */
    event CiphernodeDeregistrationRequested(
        address indexed operator,
        uint64 unlockAt
    );

    /**
     * @notice Emitted when operator active status changes
     * @param operator Address of the operator
     * @param active True if active, false if inactive
     */
    event OperatorActivationChanged(address indexed operator, bool active);

    /**
     * @notice Emitted when configuration is updated
     * @param parameter Name of the parameter
     * @param oldValue Previous value
     * @param newValue New value
     */
    event ConfigurationUpdated(
        bytes32 indexed parameter,
        uint256 oldValue,
        uint256 newValue
    );

    /**
     * @notice Emitted when an eligibility setting invalidates cached operator status.
     * @param version New eligibility-policy version.
     */
    event EligibilityConfigurationVersionUpdated(uint256 indexed version);

    /**
     * @notice Emitted when a reward distributor is authorized or revoked
     * @param distributor Address of the distributor
     * @param authorized True if authorized, false if revoked
     */
    event RewardDistributorUpdated(
        address indexed distributor,
        bool authorized
    );

    /**
     * @notice Emitted when treasury withdraws slashed funds
     * @param to Treasury address
     * @param ticketAmount Amount of slashed ticket balance withdrawn
     * @param licenseAmount Amount of slashed license bond withdrawn
     */
    event SlashedFundsWithdrawn(
        address indexed to,
        uint256 ticketAmount,
        uint256 licenseAmount
    );

    /**
     * @notice Emitted when the slashed funds treasury address is set
     * @param treasury Address of the slashed funds treasury
     */
    event SlashedFundsTreasurySet(address indexed treasury);

    /**
     * @notice Emitted when the ticket token is set
     * @param ticketToken Address of the ticket token
     */
    event TicketTokenSet(address indexed ticketToken);

    /**
     * @notice Emitted when the license token is set
     * @param licenseToken Address of the license token
     */
    event LicenseTokenSet(address indexed licenseToken);

    /**
     * @notice Emitted when governance removes license tokens that are not
     *         backing any active bond, pending exit, or slashed-fund claim.
     * @param token License token whose surplus was swept
     * @param to Slashed-funds treasury that received the surplus
     * @param amount Amount requested for transfer
     */
    event LicenseSurplusSwept(
        address indexed token,
        address indexed to,
        uint256 amount
    );

    /**
     * @notice Emitted when the registry is set
     * @param registry Address of the registry
     */
    event RegistrySet(address indexed registry);

    /// @notice Emitted whenever the slashing manager address is updated.
    event SlashingManagerUpdated(
        address indexed previous,
        address indexed next
    );

    /// @notice Emitted whenever a slashing manager gains or loses authority.
    /// @dev Replaced managers remain authorized until governance explicitly
    ///      revokes them so snapshotted E3s and open proposals can finish.
    event SlashingManagerAuthorizationUpdated(
        address indexed slashingManager,
        bool authorized
    );

    /**
     * @notice Emitted whenever a `licenseToken.safeTransfer` performed by the
     *         registry sends FEWER tokens than requested (typical of
     *         fee-on-transfer or rebasing tokens). The registry's internal
     *         accounting is decremented by the requested amount, but the
     *         recipient only receives `actualAmount`. The difference is left
     *         in the registry as an unaccounted-for surplus. Operators and
     *         monitoring infrastructure should treat any emission of this
     *         event as evidence that the configured `licenseToken` is not a
     *         well-behaved ERC-20. Rotation is permitted only after every
     *         balance denominated in the old token has been drained.
     * @param recipient The address that received the (short) transfer
     * @param expectedAmount The amount the registry intended to send
     * @param actualAmount The actual delta in registry-held balance
     */
    event LicenseTransferShortfall(
        address indexed recipient,
        uint256 expectedAmount,
        uint256 actualAmount
    );

    /**
     * @notice Emitted when the wallet that owns an operator's collateral is set.
     * @param operator Hot operator key used by the node
     * @param bondOwner Wallet that funds and controls the operator's collateral
     */
    event BondOwnerSet(address indexed operator, address indexed bondOwner);

    /**
     * @notice Emitted when the current owner proposes transferring an operator position.
     * @param operator Hot operator key used by the node
     * @param currentOwner Current wallet controlling the operator's collateral
     * @param pendingOwner Wallet that may accept ownership
     */
    event BondOwnerTransferProposed(
        address indexed operator,
        address indexed currentOwner,
        address indexed pendingOwner
    );

    // ======================
    // View Functions
    // ======================

    /**
     * @notice Get license token address
     * @return License token address
     */
    function getLicenseToken() external view returns (address);

    /**
     * @notice Total license-token obligations held by the registry.
     * @dev Covers active bonds, queued exits, and slashed funds awaiting
     *      treasury withdrawal.
     */
    function totalLicenseLiability() external view returns (uint256);

    /**
     * @notice Get ticket token address
     * @return Ticket token address
     */
    function getTicketToken() external view returns (address);

    /**
     * @notice Get operator's current ticket balance
     * @param operator Address of the operator
     * @return Current collateral balance
     */
    function getTicketBalance(address operator) external view returns (uint256);

    /**
     * @notice Get operator's current license bond
     * @param operator Address of the operator
     * @return Current license bond
     */
    function getLicenseBond(address operator) external view returns (uint256);

    /**
     * @notice Get the wallet that owns and controls an operator's collateral.
     * @dev Returns address(0) until the operator authorizes an owner.
     */
    function bondOwnerOf(address operator) external view returns (address);

    /**
     * @notice Get the wallet currently nominated to accept an operator position.
     */
    function pendingBondOwnerOf(
        address operator
    ) external view returns (address);

    /**
     * @notice Get FOLD that still counts toward an account's locked-floor collateral.
     * @dev Includes active license bond plus pending FOLD exits that remain slashable/not returned.
     * @param account Bond owner whose aggregate FOLD bond credit is queried
     * @return Active plus pending license-bond amount
     */
    function totalBonded(address account) external view returns (uint256);

    /**
     * @notice Get current ticket price
     * @return Price per ticket in collateral token units
     */
    function ticketPrice() external view returns (uint256);

    /**
     * @notice Calculate available tickets for an operator
     * @param operator Address of the operator
     * @return Number of tickets available (floor(balance / ticketPrice))
     */
    function availableTickets(address operator) external view returns (uint256);

    /**
     * @notice Check if operator is licensed
     * @param operator Address of the operator
     * @return True if operator has sufficient license bond
     */
    function isLicensed(address operator) external view returns (bool);

    /**
     * @notice Check if operator is registered
     * @param operator Address of the operator
     * @return True if operator is registered
     */
    function isRegistered(address operator) external view returns (bool);

    /**
     * @notice Check if operator is active
     * @param operator Address of the operator
     * @return True if operator is active (licensed, registered, and has min tickets)
     */
    function isActive(address operator) external view returns (bool);

    /**
     * @notice Get the number of currently active operators
     * @return Number of active operators
     */
    function numActiveOperators() external view returns (uint256);

    /// @notice Current eligibility-policy version.
    function eligibilityConfigurationVersion() external view returns (uint256);

    /**
     * @notice Re-evaluate one registered operator under the current eligibility policy.
     * @dev Permissionless so operators or governance can restore current status after
     *      an eligibility configuration update.
     */
    function refreshOperatorStatus(address operator) external;

    /**
     * @notice Re-evaluate a batch of registered operators under the current policy.
     * @dev Intended for governance/operator automation after a policy update.
     */
    function refreshOperatorStatuses(address[] calldata operators) external;

    /**
     * @notice Check if operator has deregistration in progress
     * @param operator Address of the operator
     * @return True if exit requested but not finalized
     */
    function hasExitInProgress(address operator) external view returns (bool);

    /**
     * @notice Get license bond price required
     * @return License bond price amount
     */
    function licenseRequiredBond() external view returns (uint256);

    /**
     * @notice Get minimum ticket balance required for activation
     * @return Minimum number of tickets required
     */
    function minTicketBalance() external view returns (uint256);

    /**
     * @notice Get exit delay period
     * @return Number of seconds operators must wait after requesting exit
     */
    function exitDelay() external view returns (uint64);

    /**
     * @notice Get operator's ticket balance at a specific timepoint (EIP-6372).
     * @dev The ticket token uses {block.timestamp} for its voting clock.
     * @param operator Address of the operator
     * @param blockNumber Timepoint (block.timestamp) to query
     * @return Ticket balance at the specified timepoint
     */
    function getTicketBalanceAtBlock(
        address operator,
        uint256 blockNumber
    ) external view returns (uint256);

    /**
     * @notice Get operator's total pending exit amounts
     * @param operator Address of the operator
     * @return ticket Total pending ticket balance in exit queue
     * @return license Total pending license bond in exit queue
     */
    function pendingExits(
        address operator
    ) external view returns (uint256 ticket, uint256 license);

    /**
     * @notice Preview how much an operator can currently claim
     * @param operator Address of the operator
     * @return ticket Claimable ticket balance
     * @return license Claimable license bond
     */
    function previewClaimable(
        address operator
    ) external view returns (uint256 ticket, uint256 license);

    /**
     * @notice Get slashed funds treasury address
     * @return Address where slashed funds are sent
     */
    function slashedFundsTreasury() external view returns (address);

    /**
     * @notice Get total slashed ticket balance
     * @return Amount of ticket balance slashed and available for treasury withdrawal
     */
    function slashedTicketBalance() external view returns (uint256);

    /// @notice Get slashed ticket funds reserved for retryable E3 routing.
    function reservedSlashedTicketBalance() external view returns (uint256);

    /// @notice Get the ticket wrapper whose underlying asset backs ticket slashes.
    function ticketToken() external view returns (InterfoldTicketToken);

    /**
     * @notice Get total slashed license bond
     * @return Amount of license bond slashed and available for treasury withdrawal
     */
    function slashedLicenseBond() external view returns (uint256);

    // ======================
    // Operator Functions
    // ======================

    /**
     * @notice Register an operator whose collateral is controlled by the caller.
     */
    function registerOperatorFor(address operator) external;

    /**
     * @notice Deregister an operator.
     * @dev Callable by the bond owner or by the operator as an emergency kill switch.
     */
    function deregisterOperatorFor(address operator) external;

    /**
     * @notice Increase an operator's ticket balance using the bond owner's funds.
     */
    function addTicketBalanceFor(address operator, uint256 amount) external;

    /**
     * @notice Queue ticket collateral owned by the caller for withdrawal.
     */
    function removeTicketBalanceFor(address operator, uint256 amount) external;

    /**
     * @notice Bond license tokens for an operator using the bond owner's funds.
     */
    function bondLicenseFor(address operator, uint256 amount) external;

    /**
     * @notice Queue license collateral owned by the caller for withdrawal.
     */
    function unbondLicenseFor(address operator, uint256 amount) external;

    /**
     * @notice Authorize a bond owner for the caller's operator key.
     * @dev The owner may be the operator itself. The operator may correct this
     *      choice while the position is empty; funded positions require current-owner
     *      proposal and new-owner acceptance.
     */
    function setBondOwner(address bondOwner) external;

    /**
     * @notice Propose transferring an operator position to a new bond owner.
     * @dev Only the current bond owner may propose or replace a pending transfer.
     */
    function proposeBondOwner(address operator, address newOwner) external;

    /**
     * @notice Accept a proposed operator position from its current bond owner.
     * @dev Moves ownership accounting for active and pending license collateral
     *      only when doing so preserves the previous owner's locked-FOLD coverage.
     */
    function acceptBondOwner(address operator) external;

    // ======================
    // Claim Functions
    // ======================

    /**
     * @notice Claim an operator's matured exits to its bond owner.
     */
    function claimExitsFor(
        address operator,
        uint256 maxTicketAmount,
        uint256 maxLicenseAmount
    ) external;

    // ======================
    // Slashing Functions
    // ======================

    /**
     * @notice Slash operator's ticket balance by absolute amount
     * @param operator Address of the operator to slash
     * @param amount Amount to slash
     * @param reason Reason for slashing (stored in event)
     * @dev Only callable by authorized slashing manager
     */
    function slashTicketBalance(
        address operator,
        uint256 amount,
        bytes32 reason
    ) external returns (uint256 actualAmount);

    /**
     * @notice Slash operator's license bond by absolute amount
     * @param operator Address of the operator to slash
     * @param amount Amount to slash
     * @param reason Reason for slashing (stored in event)
     * @dev Only callable by authorized slashing manager
     * @return actualAmount Amount removed from active and pending license collateral
     */
    function slashLicenseBond(
        address operator,
        uint256 amount,
        bytes32 reason
    ) external returns (uint256 actualAmount);

    /// @notice Reserve slashed ticket funds so treasury cannot withdraw them.
    /// @dev Only callable by the configured slashing manager.
    function reserveSlashedTicketFunds(uint256 amount) external;

    /// @notice Route and consume previously reserved slashed ticket funds.
    /// @dev Only callable by the configured slashing manager.
    function redirectReservedSlashedTicketFunds(
        address to,
        uint256 amount
    ) external;

    // ======================
    // Reward Distribution Functions
    // ======================
    /**
     * @notice Distribute rewards for operators to their configured bond owners.
     * @param rewardToken Reward token contract
     * @param operators Addresses of the operators to distribute rewards to
     * @param amounts Amounts of rewards to distribute to each operator
     * @dev Falls back to the supplied address when it has no configured bond owner.
     *      Only callable by authorized distributors.
     */
    function distributeRewards(
        IERC20 rewardToken,
        address[] calldata operators,
        uint256[] calldata amounts
    ) external;

    // ======================
    // Admin Functions
    // ======================

    /**
     * @notice Set ticket price
     * @param newTicketPrice New price per ticket
     * @dev Only callable by contract owner. Invalidates cached operator status.
     */
    function setTicketPrice(uint256 newTicketPrice) external;

    /**
     * @notice Set license bond price required
     * @param newLicenseRequiredBond New license bond price
     * @dev Only callable by contract owner. Invalidates cached operator status.
     */
    function setLicenseRequiredBond(uint256 newLicenseRequiredBond) external;

    /**
     * @notice Set license active BPS
     * @param newBps New license active BPS
     * @dev Only callable by contract owner. Invalidates cached operator status.
     */
    function setLicenseActiveBps(uint256 newBps) external;

    /**
     * @notice Set minimum ticket balance required for activation
     * @param newMinTicketBalance New minimum ticket balance
     * @dev Only callable by contract owner. Invalidates cached operator status.
     */
    function setMinTicketBalance(uint256 newMinTicketBalance) external;

    /**
     * @notice Set exit delay period
     * @param newExitDelay New exit delay in seconds
     * @dev Only callable by contract owner
     */
    function setExitDelay(uint64 newExitDelay) external;

    /**
     * @notice Set ticket token
     * @param newTicketToken New ticket token
     * @dev Only callable by contract owner
     */
    function setTicketToken(InterfoldTicketToken newTicketToken) external;

    /**
     * @notice Set license token
     * @param newLicenseToken New license token
     * @dev Only callable by contract owner. A nonzero token must implement
     *      `ILockAwareLicenseToken.lockedBalanceOf`.
     */
    function setLicenseToken(IERC20 newLicenseToken) external;

    /**
     * @notice Send unaccounted license-token surplus to the slashed-funds treasury.
     * @dev Never transfers active bonds, queued exits, or slashed-fund liabilities.
     *      This is the governance path for clearing donated dust before rotation.
     * @return amount Amount requested for transfer
     */
    function sweepLicenseSurplus() external returns (uint256 amount);

    /**
     * @notice Set slashed funds treasury address
     * @param newSlashedFundsTreasury New slashed funds treasury address
     * @dev Only callable by contract owner
     */
    function setSlashedFundsTreasury(address newSlashedFundsTreasury) external;

    /**
     * @notice Set registry address
     * @param newRegistry New registry contract address
     * @dev Only callable by contract owner
     */
    function setRegistry(ICiphernodeRegistry newRegistry) external;

    /**
     * @notice Set slashing manager address
     * @param newSlashingManager New slashing manager contract address
     * @dev Only callable by contract owner
     */
    function setSlashingManager(address newSlashingManager) external;

    /**
     * @notice Revoke a non-current slashing manager after every E3 and proposal
     *         that depends on it has reached a terminal state.
     * @param oldSlashingManager Manager whose authority should be removed
     */
    function revokeSlashingManager(address oldSlashingManager) external;

    /**
     * @notice Whether a manager may slash collateral or route reserved slash funds.
     */
    function isAuthorizedSlashingManager(
        address candidate
    ) external view returns (bool);

    /**
     * @notice Number of currently authorized slashing managers.
     */
    function authorizedSlashingManagerCount() external view returns (uint256);

    /**
     * @notice Authorized slashing manager at `index`.
     */
    function authorizedSlashingManagerAt(
        uint256 index
    ) external view returns (address);

    /**
     * @notice Set reward distributor address
     * @param newRewardDistributor New reward distributor address
     * @dev Only callable by contract owner
     */
    function setRewardDistributor(address newRewardDistributor) external;

    /**
     * @notice Revoke reward distributor authorization
     * @param distributor Address to revoke
     * @dev Only callable by contract owner
     */
    function revokeRewardDistributor(address distributor) external;

    /**
     * @notice Withdraw slashed funds to treasury
     * @param ticketAmount Amount of slashed ticket balance to withdraw
     * @param licenseAmount Amount of slashed license bond to withdraw
     * @dev Only callable by contract owner, sends to treasury address
     */
    function withdrawSlashedFunds(
        uint256 ticketAmount,
        uint256 licenseAmount
    ) external;
}
