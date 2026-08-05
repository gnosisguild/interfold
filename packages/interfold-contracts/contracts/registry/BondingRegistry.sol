// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import {
    Ownable2StepUpgradeable
} from "@openzeppelin/contracts-upgradeable/access/Ownable2StepUpgradeable.sol";
import {
    ReentrancyGuardUpgradeable
} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardUpgradeable.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {
    SafeERC20
} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { Math } from "@openzeppelin/contracts/utils/math/Math.sol";
import {
    IERC165
} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import { ExitQueueLib } from "../lib/ExitQueueLib.sol";

import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { ICiphernodeRegistry } from "../interfaces/ICiphernodeRegistry.sol";
import {
    ILockAwareLicenseToken
} from "../interfaces/ILockAwareLicenseToken.sol";
import { ISlashingManager } from "../interfaces/ISlashingManager.sol";
import { InterfoldTicketToken } from "../token/InterfoldTicketToken.sol";

/**
 * @title BondingRegistry
 * @notice Implementation of the bonding registry managing operator ticket balances and license bonds
 * @dev Handles deposits, withdrawals, slashing, exits, and integrates with registry and slashing manager
 */
// solhint-disable-next-line max-states-count
contract BondingRegistry is
    IBondingRegistry,
    Ownable2StepUpgradeable,
    ReentrancyGuardUpgradeable
{
    using SafeERC20 for IERC20;
    using ExitQueueLib for ExitQueueLib.ExitQueueState;

    // ======================
    // Constants
    // ======================

    /// @dev Reason code for ticket balance deposits
    bytes32 private constant REASON_DEPOSIT = bytes32("DEPOSIT");

    /// @dev Reason code for ticket balance withdrawals
    bytes32 private constant REASON_WITHDRAW = bytes32("WITHDRAW");

    /// @dev Reason code for license bond operations
    bytes32 private constant REASON_BOND = bytes32("BOND");

    /// @dev Reason code for license unbond operations
    bytes32 private constant REASON_UNBOND = bytes32("UNBOND");

    // ======================
    // Storage
    // ======================

    /// @notice Ticket token (tFOLD with underlying USDC) used for collateral
    InterfoldTicketToken public ticketToken;

    /// @notice License token (FOLD) required for operator registration
    IERC20 public licenseToken;

    /// @notice Registry contract for managing committee membership
    ICiphernodeRegistry public registry;

    /// @notice Address authorized to perform slashing operations
    address public slashingManager;

    /// @notice Addresses authorized to distribute rewards to operators
    /// @dev Multiple contracts (Interfold, E3RefundManager) need to distribute rewards.
    ///      Each authorized distributor must approve this contract for the reward token.
    mapping(address distributor => bool authorized)
        public authorizedDistributors;

    /// @notice Current count of authorized distributors. Bounded by
    ///         {MAX_AUTHORIZED_DISTRIBUTORS}.
    uint256 public authorizedDistributorCount;

    /// @notice Hard cap on the number of authorized reward distributors so
    ///         downstream payout loops stay bounded.
    uint256 public constant MAX_AUTHORIZED_DISTRIBUTORS = 32;

    /// @notice Minimum permitted value for {exitDelay}. Set to one day so
    ///         an attacker cannot drain stake immediately after winning ownership.
    uint64 public constant MIN_EXIT_DELAY = 1 days;

    /// @notice Maximum permitted value for {exitDelay}. Caps the freeze
    ///         duration so operators retain a meaningful exit path.
    uint64 public constant MAX_EXIT_DELAY = 90 days; // duration in seconds; not calendar-aware

    /// @notice Basis-points denominator (100% = 10_000 bps).
    uint256 internal constant BPS_BASE = 10_000;

    /// @notice Treasury address that receives slashed funds
    address public slashedFundsTreasury;

    /// @notice Price per ticket in ticket token units
    uint256 public ticketPrice;

    /// @notice Minimum license bond required for initial registration
    uint256 public licenseRequiredBond;

    /// @notice Minimum number of tickets required to maintain active status
    uint256 public minTicketBalance;

    /// @notice Time delay in seconds before exits can be claimed
    uint64 public exitDelay;

    /// @notice Percentage (in basis points) of license bond that must remain bonded to stay active
    /// @dev Default 8000 = 80%. Allows operators to unbond up to 20% while remaining active
    uint256 public licenseActiveBps;

    /// @notice Number of currently active operators
    uint256 public numActiveOperators;

    /// @notice Operator state data structure
    /// @param licenseBond Amount of license tokens currently bonded
    /// @param exitUnlocksAt Timestamp when pending exit can be claimed
    /// @param registered Whether operator is registered in the protocol
    /// @param exitRequested Whether operator has requested to exit
    /// @param active Whether operator meets all requirements for active status
    struct Operator {
        uint256 licenseBond;
        uint64 exitUnlocksAt;
        bool registered;
        bool exitRequested;
        bool active;
        uint256 eligibilityVersion;
    }

    /// @notice Maps operator address to their state data
    mapping(address operator => Operator data) internal operators;

    /// @notice Total slashed ticket balance available for treasury withdrawal
    uint256 public slashedTicketBalance;

    /// @notice Total slashed license bond available for treasury withdrawal
    uint256 public slashedLicenseBond;

    // ======================
    // Exit Queue library state
    // ======================

    /// @dev Internal state for managing exit queue of tickets and licenses
    ExitQueueLib.ExitQueueState private _exits;

    /// @notice Version of the current operator-eligibility policy.
    /// @dev Every eligibility update advances the version and resets
    ///      {numActiveOperators}. Operators fail closed until refreshed.
    uint256 public eligibilityConfigurationVersion;

    /// @notice Slashed tickets committed to retryable E3 refund routes.
    uint256 public reservedSlashedTicketBalance;

    /// @notice Slashing managers that may finish snapshotted E3 lifecycles.
    /// @dev Rotating the current manager does not revoke its predecessor.
    address[] internal _authorizedSlashingManagers;

    /// @dev One-based index into {_authorizedSlashingManagers}; zero means unauthorized.
    mapping(address manager => uint256 indexPlusOne)
        internal _authorizedSlashingManagerIndex;

    /// @notice Maximum number of concurrently authorized slashing managers.
    uint256 public constant MAX_AUTHORIZED_SLASHING_MANAGERS = 32;

    /// @inheritdoc IBondingRegistry
    uint256 public totalLicenseLiability;

    // ======================
    // Modifiers
    // ======================

    /// @dev Restricts function access to current or retained historical managers.
    modifier onlyAuthorizedSlashingManager() {
        if (_authorizedSlashingManagerIndex[msg.sender] == 0) {
            revert Unauthorized();
        }
        _;
    }

    /// @dev Restricts function access to authorized reward distributors
    modifier onlyAuthorizedDistributor() {
        require(authorizedDistributors[msg.sender], OnlyRewardDistributor());
        _;
    }

    /// @dev Reverts if operator has an exit in progress that hasn't unlocked yet
    /// @param operator Address of the operator to check
    modifier noExitInProgress(address operator) {
        Operator memory op = operators[operator];
        if (op.exitRequested && block.timestamp < op.exitUnlocksAt) {
            revert ExitInProgress();
        }
        _;
    }

    /// @dev Keeps active and already-queued collateral available while any
    ///      financial slash proposal against the operator is unresolved.
    modifier noOpenSlashProposal(address operator) {
        uint256 len = _authorizedSlashingManagers.length;
        for (uint256 i = 0; i < len; i++) {
            if (
                ISlashingManager(_authorizedSlashingManagers[i])
                    .hasOpenSlashProposal(operator)
            ) {
                revert OperatorUnderSlash();
            }
        }
        _;
    }

    /// @dev Restricts collateral and lifecycle actions to the operator's bond owner.
    modifier onlyBondOwner(address operator) {
        _checkBondOwner(operator);
        _;
    }

    /// @dev Allows the collateral owner or the hot operator key to stop participation.
    modifier onlyBondOwnerOrOperator(address operator) {
        if (msg.sender != operator) {
            _checkBondOwner(operator);
        }
        _;
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Initialization                       //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Locks the implementation; initialize via the proxy.
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the bonding registry contract
    /// @param _owner Address that will own the contract
    /// @param _ticketToken Ticket token contract for collateral
    /// @param _licenseToken License token contract for bonding
    /// @param _registry Ciphernode registry contract
    /// @param _slashedFundsTreasury Address to receive slashed funds
    /// @param _ticketPrice Initial price per ticket
    /// @param _licenseRequiredBond Initial required license bond for registration
    /// @param _minTicketBalance Initial minimum ticket balance for activation
    /// @param _exitDelay Initial exit delay period in seconds
    function initialize(
        address _owner,
        InterfoldTicketToken _ticketToken,
        IERC20 _licenseToken,
        ICiphernodeRegistry _registry,
        address _slashedFundsTreasury,
        uint256 _ticketPrice,
        uint256 _licenseRequiredBond,
        uint256 _minTicketBalance,
        uint64 _exitDelay
    ) public initializer {
        __Ownable_init(msg.sender);
        __ReentrancyGuard_init();
        setTicketToken(_ticketToken);
        setLicenseToken(_licenseToken);
        setRegistry(_registry);
        setSlashedFundsTreasury(_slashedFundsTreasury);
        setTicketPrice(_ticketPrice);
        setLicenseRequiredBond(_licenseRequiredBond);
        setMinTicketBalance(_minTicketBalance);
        setExitDelay(_exitDelay);
        setLicenseActiveBps(8_000);
        if (_owner != owner()) _transferOwnership(_owner);
    }

    // ======================
    // View Functions
    // ======================

    /// @inheritdoc IBondingRegistry
    function getLicenseToken() external view returns (address) {
        return address(licenseToken);
    }

    /// @inheritdoc IBondingRegistry
    function getTicketToken() external view returns (address) {
        return address(ticketToken);
    }

    /// @inheritdoc IBondingRegistry
    function getTicketBalance(
        address operator
    ) external view returns (uint256) {
        return ticketToken.balanceOf(operator);
    }

    /// @inheritdoc IBondingRegistry
    function getLicenseBond(address operator) external view returns (uint256) {
        return operators[operator].licenseBond;
    }

    /// @inheritdoc IBondingRegistry
    function bondOwnerOf(address operator) public view returns (address) {
        return _bondOwnerOf[operator];
    }

    /// @inheritdoc IBondingRegistry
    function pendingBondOwnerOf(
        address operator
    ) external view returns (address) {
        return _pendingBondOwnerOf[operator];
    }

    /// @inheritdoc IBondingRegistry
    function totalBonded(address account) external view returns (uint256) {
        return _bondedByOwner[account];
    }

    /// @inheritdoc IBondingRegistry
    function availableTickets(
        address operator
    ) external view returns (uint256) {
        return ticketToken.balanceOf(operator) / ticketPrice;
    }

    /// @notice Get operator's ticket balance at a specific timepoint (EIP-6372).
    /// @dev The ticket token uses {block.timestamp} (mode=timestamp) for its voting clock, so
    ///      `blockNumber` is in fact a unix timestamp. Name is preserved for storage/event
    ///      compatibility.
    /// @param operator Address of the operator
    /// @param blockNumber Timepoint (block.timestamp) to query
    /// @return Ticket balance at the specified timepoint
    function getTicketBalanceAtBlock(
        address operator,
        uint256 blockNumber
    ) external view returns (uint256) {
        return ticketToken.getPastVotes(operator, blockNumber);
    }

    /// @notice Get operator's total pending exit amounts
    /// @param operator Address of the operator
    /// @return ticket Total pending ticket balance in exit queue
    /// @return license Total pending license bond in exit queue
    function pendingExits(
        address operator
    ) external view returns (uint256 ticket, uint256 license) {
        (ticket, license) = _exits.getPendingAmounts(operator);
    }

    /// @notice Preview how much an operator can currently claim
    /// @param operator Address of the operator
    /// @return ticket Claimable ticket balance
    /// @return license Claimable license bond
    function previewClaimable(
        address operator
    ) external view returns (uint256 ticket, uint256 license) {
        (ticket, license) = _exits.previewClaimableAmounts(operator);
    }

    /// @inheritdoc IBondingRegistry
    function isLicensed(address operator) external view returns (bool) {
        return operators[operator].licenseBond >= _minLicenseBond();
    }

    /// @inheritdoc IBondingRegistry
    function isRegistered(address operator) external view returns (bool) {
        return operators[operator].registered;
    }

    /// @inheritdoc IBondingRegistry
    function isActive(address operator) external view returns (bool) {
        Operator storage op = operators[operator];
        return
            op.eligibilityVersion == eligibilityConfigurationVersion &&
            op.active;
    }

    /// @inheritdoc IBondingRegistry
    function refreshOperatorStatus(address operator) public {
        require(operators[operator].registered, NotRegistered());
        _updateOperatorStatus(operator);
    }

    /// @inheritdoc IBondingRegistry
    function refreshOperatorStatuses(address[] calldata operatorList) external {
        uint256 len = operatorList.length;
        for (uint256 i = 0; i < len; i++) {
            refreshOperatorStatus(operatorList[i]);
        }
    }

    /// @inheritdoc IBondingRegistry
    function hasExitInProgress(address operator) external view returns (bool) {
        Operator memory op = operators[operator];
        return op.exitRequested && block.timestamp < op.exitUnlocksAt;
    }

    /// @inheritdoc IBondingRegistry
    function isAuthorizedSlashingManager(
        address candidate
    ) external view returns (bool) {
        return _authorizedSlashingManagerIndex[candidate] != 0;
    }

    /// @inheritdoc IBondingRegistry
    function authorizedSlashingManagerCount() external view returns (uint256) {
        return _authorizedSlashingManagers.length;
    }

    /// @inheritdoc IBondingRegistry
    function authorizedSlashingManagerAt(
        uint256 index
    ) external view returns (address) {
        return _authorizedSlashingManagers[index];
    }

    // ======================
    // Operator Functions
    // ======================

    /// @inheritdoc IBondingRegistry
    function setBondOwner(address bondOwner) external {
        require(bondOwner != address(0), ZeroAddress());

        address currentOwner = _bondOwnerOf[msg.sender];
        if (currentOwner != address(0)) {
            (uint256 pendingTicket, uint256 pendingLicense) = _exits
                .getPendingAmounts(msg.sender);
            Operator storage op = operators[msg.sender];
            if (
                op.registered ||
                op.licenseBond != 0 ||
                pendingLicense != 0 ||
                ticketToken.balanceOf(msg.sender) != 0 ||
                pendingTicket != 0
            ) {
                revert BondOwnerAlreadySet(msg.sender, currentOwner);
            }
        }

        delete _pendingBondOwnerOf[msg.sender];
        _bondOwnerOf[msg.sender] = bondOwner;
        emit BondOwnerSet(msg.sender, bondOwner);
    }

    /// @inheritdoc IBondingRegistry
    function proposeBondOwner(
        address operator,
        address newOwner
    ) external onlyBondOwner(operator) {
        require(newOwner != address(0), ZeroAddress());
        _pendingBondOwnerOf[operator] = newOwner;
        emit BondOwnerTransferProposed(operator, msg.sender, newOwner);
    }

    /// @inheritdoc IBondingRegistry
    function acceptBondOwner(address operator) external {
        require(msg.sender == _pendingBondOwnerOf[operator], Unauthorized());

        address previousOwner = bondOwnerOf(operator);
        (, uint256 pendingLicense) = _exits.getPendingAmounts(operator);
        uint256 delegatedBond = operators[operator].licenseBond +
            pendingLicense;

        if (delegatedBond != 0) {
            uint256 remainingBonded = _bondedByOwner[previousOwner] -
                delegatedBond;
            uint256 lockedBalance = _lockedBalanceOf(
                licenseToken,
                previousOwner
            );
            uint256 controlledBalance = licenseToken.balanceOf(previousOwner) +
                remainingBonded;
            if (lockedBalance > controlledBalance) {
                revert BondOwnerTransferViolatesLock(
                    previousOwner,
                    lockedBalance,
                    controlledBalance
                );
            }
        }

        delete _pendingBondOwnerOf[operator];
        _bondOwnerOf[operator] = msg.sender;
        _bondedByOwner[previousOwner] -= delegatedBond;
        _bondedByOwner[msg.sender] += delegatedBond;

        emit BondOwnerSet(operator, msg.sender);
    }

    /// @inheritdoc IBondingRegistry
    function registerOperatorFor(
        address operator
    ) external noExitInProgress(operator) onlyBondOwner(operator) {
        _registerOperator(operator);
    }

    function _registerOperator(address operator) internal {
        // Clear previous exit request
        if (operators[operator].exitRequested) {
            operators[operator].exitRequested = false;
            operators[operator].exitUnlocksAt = 0;
        }

        require(slashingManager != address(0), ZeroAddress());
        require(!_isOperatorBanned(operator), CiphernodeBanned());
        require(!operators[operator].registered, AlreadyRegistered());
        require(
            operators[operator].licenseBond >= licenseRequiredBond,
            NotLicensed()
        );

        operators[operator].registered = true;

        // CiphernodeRegistry already emits an event when a ciphernode is added
        registry.addCiphernode(operator);

        _updateOperatorStatus(operator);
    }

    /// @inheritdoc IBondingRegistry
    function deregisterOperatorFor(
        address operator
    )
        external
        noExitInProgress(operator)
        noOpenSlashProposal(operator)
        onlyBondOwnerOrOperator(operator)
    {
        _deregisterOperator(operator);
    }

    function _deregisterOperator(address operator) internal {
        Operator storage op = operators[operator];
        require(op.registered, NotRegistered());

        op.registered = false;
        op.exitRequested = true;
        op.exitUnlocksAt = uint64(block.timestamp) + exitDelay;

        uint256 ticketOut = ticketToken.balanceOf(operator);
        uint256 licenseOut = op.licenseBond;
        if (ticketOut != 0) {
            ticketToken.burnTickets(operator, ticketOut);
            emit TicketBalanceUpdated(
                operator,
                -int256(ticketOut),
                0,
                REASON_WITHDRAW
            );
        }
        if (licenseOut != 0) {
            op.licenseBond = 0;
            emit LicenseBondUpdated(
                operator,
                -int256(licenseOut),
                0,
                REASON_UNBOND
            );
        }

        if (ticketOut != 0 || licenseOut != 0) {
            _exits.queueAssetsForExit(
                operator,
                exitDelay,
                ticketOut,
                licenseOut
            );
        }

        // CiphernodeRegistry already emits an event when a ciphernode is removed
        registry.removeCiphernode(operator);

        emit CiphernodeDeregistrationRequested(operator, op.exitUnlocksAt);
        _updateOperatorStatus(operator);
    }

    /// @inheritdoc IBondingRegistry
    function addTicketBalanceFor(
        address operator,
        uint256 amount
    ) external noExitInProgress(operator) onlyBondOwner(operator) {
        _addTicketBalance(operator, amount);
    }

    function _addTicketBalance(address operator, uint256 amount) internal {
        require(amount != 0, ZeroAmount());
        require(operators[operator].registered, NotRegistered());

        ticketToken.depositFrom(msg.sender, operator, amount);

        emit TicketBalanceUpdated(
            operator,
            int256(amount),
            ticketToken.balanceOf(operator),
            REASON_DEPOSIT
        );

        _updateOperatorStatus(operator);
    }

    /// @inheritdoc IBondingRegistry
    function removeTicketBalanceFor(
        address operator,
        uint256 amount
    )
        external
        noExitInProgress(operator)
        noOpenSlashProposal(operator)
        onlyBondOwner(operator)
    {
        _removeTicketBalance(operator, amount);
    }

    function _removeTicketBalance(address operator, uint256 amount) internal {
        require(amount != 0, ZeroAmount());
        require(operators[operator].registered, NotRegistered());
        require(
            ticketToken.balanceOf(operator) >= amount,
            InsufficientBalance()
        );

        ticketToken.burnTickets(operator, amount);
        _exits.queueTicketsForExit(operator, exitDelay, amount);

        emit TicketBalanceUpdated(
            operator,
            -int256(amount),
            ticketToken.balanceOf(operator),
            REASON_WITHDRAW
        );

        _updateOperatorStatus(operator);
    }

    /// @inheritdoc IBondingRegistry
    function bondLicenseFor(
        address operator,
        uint256 amount
    ) external nonReentrant noExitInProgress(operator) {
        _bondLicense(operator, amount);
    }

    /// @inheritdoc IBondingRegistry
    function unbondLicenseFor(
        address operator,
        uint256 amount
    )
        external
        nonReentrant
        noExitInProgress(operator)
        noOpenSlashProposal(operator)
        onlyBondOwner(operator)
    {
        _unbondLicense(operator, amount);
    }

    function _unbondLicense(address operator, uint256 amount) internal {
        require(amount != 0, ZeroAmount());
        require(
            operators[operator].licenseBond >= amount,
            InsufficientBalance()
        );

        operators[operator].licenseBond -= amount;
        _exits.queueLicensesForExit(operator, exitDelay, amount);

        emit LicenseBondUpdated(
            operator,
            -int256(amount),
            operators[operator].licenseBond,
            REASON_UNBOND
        );

        _updateOperatorStatus(operator);
    }

    // ======================
    // Claim Functions
    // ======================

    /// @inheritdoc IBondingRegistry
    function claimExitsFor(
        address operator,
        uint256 maxTicketAmount,
        uint256 maxLicenseAmount
    )
        external
        nonReentrant
        noOpenSlashProposal(operator)
        onlyBondOwner(operator)
    {
        _claimExits(operator, maxTicketAmount, maxLicenseAmount);
    }

    function _claimExits(
        address operator,
        uint256 maxTicketAmount,
        uint256 maxLicenseAmount
    ) internal {
        (uint256 ticketClaim, uint256 licenseClaim) = _exits.claimAssets(
            operator,
            maxTicketAmount,
            maxLicenseAmount
        );
        require(ticketClaim > 0 || licenseClaim > 0, ExitNotReady());

        address bondOwner = bondOwnerOf(operator);
        if (ticketClaim > 0) ticketToken.payout(bondOwner, ticketClaim);
        if (licenseClaim > 0) {
            _decreaseDelegatedBond(operator, licenseClaim);
            totalLicenseLiability -= licenseClaim;
            _safeTransferLicenseWithDeltaCheck(bondOwner, licenseClaim);
        }
    }

    // ======================
    // Slashing Functions
    // ======================

    /// @inheritdoc IBondingRegistry
    function slashTicketBalance(
        address operator,
        uint256 requestedSlashAmount,
        bytes32 slashReason
    ) external onlyAuthorizedSlashingManager returns (uint256) {
        require(requestedSlashAmount != 0, ZeroAmount());

        (uint256 pendingTicketBalance, ) = _exits.getPendingAmounts(operator);
        uint256 activeBalance = ticketToken.balanceOf(operator);
        uint256 totalAvailableBalance = activeBalance + pendingTicketBalance;

        uint256 actualSlashAmount = Math.min(
            requestedSlashAmount,
            totalAvailableBalance
        );

        if (actualSlashAmount == 0) {
            return 0;
        }

        // Slash from active balance first
        uint256 slashedFromActiveBalance = Math.min(
            actualSlashAmount,
            activeBalance
        );
        if (slashedFromActiveBalance > 0) {
            ticketToken.burnTickets(operator, slashedFromActiveBalance);
        }

        // Slash remaining amount from pending queue
        uint256 remainingToSlash = actualSlashAmount - slashedFromActiveBalance;
        if (remainingToSlash > 0) {
            (uint256 pendingSlashed, ) = _exits.slashPendingAssets(
                operator,
                remainingToSlash,
                0, // licenseAmount
                true
            );
            require(pendingSlashed == remainingToSlash, InsufficientBalance());
        }

        slashedTicketBalance += actualSlashAmount;
        emit TicketBalanceUpdated(
            operator,
            -int256(actualSlashAmount),
            ticketToken.balanceOf(operator),
            slashReason
        );

        _updateOperatorStatus(operator);

        return actualSlashAmount;
    }

    /// @inheritdoc IBondingRegistry
    function slashLicenseBond(
        address operator,
        uint256 requestedSlashAmount,
        bytes32 slashReason
    ) external onlyAuthorizedSlashingManager nonReentrant returns (uint256) {
        require(requestedSlashAmount != 0, ZeroAmount());

        Operator storage operatorData = operators[operator];
        (, uint256 pendingLicenseBalance) = _exits.getPendingAmounts(operator);
        uint256 totalAvailableBalance = operatorData.licenseBond +
            pendingLicenseBalance;
        uint256 actualSlashAmount = Math.min(
            requestedSlashAmount,
            totalAvailableBalance
        );

        if (actualSlashAmount == 0) return 0;

        uint256 activeSlashAmount = Math.min(
            actualSlashAmount,
            operatorData.licenseBond
        );
        if (activeSlashAmount != 0) {
            operatorData.licenseBond -= activeSlashAmount;
        }

        uint256 remainingSlashAmount = actualSlashAmount - activeSlashAmount;
        if (remainingSlashAmount != 0) {
            (, uint256 pendingSlashed) = _exits.slashPendingAssets(
                operator,
                0,
                remainingSlashAmount,
                true
            );
            require(
                pendingSlashed == remainingSlashAmount,
                InsufficientBalance()
            );
        }

        _decreaseDelegatedBond(operator, actualSlashAmount);
        slashedLicenseBond += actualSlashAmount;
        emit LicenseBondUpdated(
            operator,
            -int256(actualSlashAmount),
            operatorData.licenseBond,
            slashReason
        );

        _updateOperatorStatus(operator);
        return actualSlashAmount;
    }

    /// @inheritdoc IBondingRegistry
    function reserveSlashedTicketFunds(
        uint256 amount
    ) external onlyAuthorizedSlashingManager {
        require(amount > 0, ZeroAmount());
        require(
            amount <= slashedTicketBalance - reservedSlashedTicketBalance,
            InsufficientBalance()
        );
        reservedSlashedTicketBalance += amount;
    }

    /// @inheritdoc IBondingRegistry
    function redirectReservedSlashedTicketFunds(
        address to,
        uint256 amount
    ) external onlyAuthorizedSlashingManager {
        require(to != address(0), ZeroAddress());
        require(amount > 0, ZeroAmount());
        require(amount <= reservedSlashedTicketBalance, InsufficientBalance());

        reservedSlashedTicketBalance -= amount;
        slashedTicketBalance -= amount;
        ticketToken.payout(to, amount);
    }

    // ======================
    // Reward Distribution Functions
    // ======================

    /// @inheritdoc IBondingRegistry
    function distributeRewards(
        IERC20 rewardToken,
        address[] calldata recipients,
        uint256[] calldata amounts
    ) external onlyAuthorizedDistributor {
        require(recipients.length == amounts.length, ArrayLengthMismatch());

        uint256 len = recipients.length;
        for (uint256 i = 0; i < len; i++) {
            if (amounts[i] > 0) {
                address recipient = bondOwnerOf(recipients[i]);
                if (recipient == address(0)) recipient = recipients[i];
                rewardToken.safeTransferFrom(msg.sender, recipient, amounts[i]);
            }
        }
    }

    // ======================
    // Admin Functions
    // ======================

    /// @inheritdoc IBondingRegistry
    function setTicketPrice(uint256 newTicketPrice) public onlyOwner {
        require(newTicketPrice != 0, InvalidConfiguration());

        uint256 oldValue = ticketPrice;
        if (oldValue == newTicketPrice) return;
        ticketPrice = newTicketPrice;
        _invalidateEligibilityStatuses();

        emit ConfigurationUpdated("ticketPrice", oldValue, newTicketPrice);
    }

    /// @inheritdoc IBondingRegistry
    function setLicenseRequiredBond(
        uint256 newLicenseRequiredBond
    ) public onlyOwner {
        require(newLicenseRequiredBond != 0, InvalidConfiguration());

        uint256 oldValue = licenseRequiredBond;
        if (oldValue == newLicenseRequiredBond) return;
        licenseRequiredBond = newLicenseRequiredBond;
        _invalidateEligibilityStatuses();

        emit ConfigurationUpdated(
            "licenseRequiredBond",
            oldValue,
            newLicenseRequiredBond
        );
    }

    /// @inheritdoc IBondingRegistry
    function setLicenseActiveBps(uint256 newBps) public onlyOwner {
        require(newBps > 0 && newBps <= BPS_BASE, InvalidConfiguration());

        uint256 oldValue = licenseActiveBps;
        if (oldValue == newBps) return;
        licenseActiveBps = newBps;
        _invalidateEligibilityStatuses();

        emit ConfigurationUpdated("licenseActiveBps", oldValue, newBps);
    }

    /// @inheritdoc IBondingRegistry
    function setMinTicketBalance(uint256 newMinTicketBalance) public onlyOwner {
        require(newMinTicketBalance != 0, InvalidConfiguration());
        uint256 oldValue = minTicketBalance;
        if (oldValue == newMinTicketBalance) return;
        minTicketBalance = newMinTicketBalance;
        _invalidateEligibilityStatuses();

        emit ConfigurationUpdated(
            "minTicketBalance",
            oldValue,
            newMinTicketBalance
        );
    }

    /// @inheritdoc IBondingRegistry
    function setExitDelay(uint64 newExitDelay) public onlyOwner {
        // bound the configurable exit delay so a malicious owner cannot
        // instantly drain operator stake (delay too short) or permanently
        // freeze withdrawals (delay too long).
        require(
            newExitDelay >= MIN_EXIT_DELAY && newExitDelay <= MAX_EXIT_DELAY,
            ExitDelayOutOfBounds(newExitDelay)
        );
        uint256 oldValue = uint256(exitDelay);
        exitDelay = newExitDelay;

        emit ConfigurationUpdated("exitDelay", oldValue, uint256(newExitDelay));
    }

    /// @inheritdoc IBondingRegistry
    function setSlashedFundsTreasury(
        address newSlashedFundsTreasury
    ) public onlyOwner {
        require(newSlashedFundsTreasury != address(0), ZeroAddress());
        slashedFundsTreasury = newSlashedFundsTreasury;
        emit SlashedFundsTreasurySet(newSlashedFundsTreasury);
    }

    /// @inheritdoc IBondingRegistry
    function setTicketToken(
        InterfoldTicketToken newTicketToken
    ) public onlyOwner {
        address next = address(newTicketToken);
        if (next.code.length == 0) revert InvalidBondingAsset(next);

        InterfoldTicketToken current = ticketToken;
        if (address(current) != address(0) && current != newTicketToken) {
            uint256 liabilities = current.totalSupply() +
                current.payableBalance();
            if (liabilities != 0) {
                revert OutstandingAssetLiabilities(
                    address(current),
                    liabilities
                );
            }
        }
        ticketToken = newTicketToken;
        emit TicketTokenSet(next);
    }

    /// @inheritdoc IBondingRegistry
    function setLicenseToken(IERC20 newLicenseToken) public onlyOwner {
        address next = address(newLicenseToken);
        IERC20 current = licenseToken;

        // BondingRegistry is deployed before FOLD because FOLD stores the
        // registry address immutably. Permit only that initial zero placeholder.
        if (next == address(0)) {
            if (address(current) != address(0)) {
                revert InvalidBondingAsset(next);
            }
        } else if (next.code.length == 0) {
            revert InvalidBondingAsset(next);
        }

        if (address(current) != address(0) && current != newLicenseToken) {
            uint256 liabilities = current.balanceOf(address(this));
            if (liabilities != 0) {
                revert OutstandingAssetLiabilities(
                    address(current),
                    liabilities
                );
            }
        }
        if (next != address(0)) {
            _lockedBalanceOf(newLicenseToken, address(this));
        }
        licenseToken = newLicenseToken;
        emit LicenseTokenSet(next);
    }

    /// @inheritdoc IBondingRegistry
    function sweepLicenseSurplus() external onlyOwner returns (uint256 amount) {
        IERC20 current = licenseToken;
        uint256 balance = current.balanceOf(address(this));
        uint256 liabilities = totalLicenseLiability;
        if (balance <= liabilities) return 0;

        amount = balance - liabilities;
        address treasury = slashedFundsTreasury;
        _safeTransferLicenseWithDeltaCheck(treasury, amount);
        emit LicenseSurplusSwept(address(current), treasury, amount);
    }

    /// @inheritdoc IBondingRegistry
    function setRegistry(ICiphernodeRegistry newRegistry) public onlyOwner {
        registry = newRegistry;
        emit RegistrySet(address(newRegistry));
    }

    /// @inheritdoc IBondingRegistry
    function setSlashingManager(address newSlashingManager) public onlyOwner {
        require(newSlashingManager != address(0), ZeroAddress());
        if (_authorizedSlashingManagerIndex[newSlashingManager] == 0) {
            require(
                _authorizedSlashingManagers.length <
                    MAX_AUTHORIZED_SLASHING_MANAGERS,
                InvalidConfiguration()
            );
            _authorizedSlashingManagers.push(newSlashingManager);
            _authorizedSlashingManagerIndex[
                newSlashingManager
            ] = _authorizedSlashingManagers.length;
            emit SlashingManagerAuthorizationUpdated(newSlashingManager, true);
        }
        address oldValue = slashingManager;
        slashingManager = newSlashingManager;
        emit SlashingManagerUpdated(oldValue, newSlashingManager);
    }

    /// @inheritdoc IBondingRegistry
    function revokeSlashingManager(
        address oldSlashingManager
    ) external onlyOwner {
        require(oldSlashingManager != slashingManager, InvalidConfiguration());
        uint256 indexPlusOne = _authorizedSlashingManagerIndex[
            oldSlashingManager
        ];
        if (indexPlusOne == 0) revert Unauthorized();

        uint256 index = indexPlusOne - 1;
        uint256 lastIndex = _authorizedSlashingManagers.length - 1;
        if (index != lastIndex) {
            address moved = _authorizedSlashingManagers[lastIndex];
            _authorizedSlashingManagers[index] = moved;
            _authorizedSlashingManagerIndex[moved] = index + 1;
        }
        _authorizedSlashingManagers.pop();
        delete _authorizedSlashingManagerIndex[oldSlashingManager];
        emit SlashingManagerAuthorizationUpdated(oldSlashingManager, false);
    }

    /// @notice Disabled. Reverts unconditionally.
    function renounceOwnership() public view override onlyOwner {
        revert RenounceOwnershipDisabled();
    }

    /// @notice Authorizes an address to distribute rewards
    /// @dev Only callable by owner. Supports multiple authorized distributors (Interfold + E3RefundManager)
    /// @param newRewardDistributor Address to authorize as reward distributor
    function setRewardDistributor(
        address newRewardDistributor
    ) public onlyOwner {
        require(newRewardDistributor != address(0), ZeroAddress());
        // hard cap on the number of authorized reward distributors so
        // payout fan-out loops in downstream consumers stay bounded.
        if (!authorizedDistributors[newRewardDistributor]) {
            require(
                authorizedDistributorCount < MAX_AUTHORIZED_DISTRIBUTORS,
                MaxAuthorizedDistributors()
            );
            authorizedDistributorCount++;
        }
        authorizedDistributors[newRewardDistributor] = true;
        emit RewardDistributorUpdated(newRewardDistributor, true);
    }

    /// @notice Revokes reward distributor authorization
    /// @dev Only callable by owner
    /// @param distributor Address to revoke
    function revokeRewardDistributor(address distributor) public onlyOwner {
        if (authorizedDistributors[distributor]) {
            authorizedDistributorCount--;
        }
        authorizedDistributors[distributor] = false;
        emit RewardDistributorUpdated(distributor, false);
    }

    /// @inheritdoc IBondingRegistry
    function withdrawSlashedFunds(
        uint256 ticketAmount,
        uint256 licenseAmount
    ) public onlyOwner {
        require(
            ticketAmount <= slashedTicketBalance - reservedSlashedTicketBalance,
            ReservedSlashedFunds()
        );
        require(licenseAmount <= slashedLicenseBond, InsufficientBalance());

        if (ticketAmount > 0) {
            slashedTicketBalance -= ticketAmount;
            ticketToken.payout(slashedFundsTreasury, ticketAmount);
        }

        if (licenseAmount > 0) {
            slashedLicenseBond -= licenseAmount;
            totalLicenseLiability -= licenseAmount;
            _safeTransferLicenseWithDeltaCheck(
                slashedFundsTreasury,
                licenseAmount
            );
        }

        emit SlashedFundsWithdrawn(
            slashedFundsTreasury,
            ticketAmount,
            licenseAmount
        );
    }

    // ======================
    // Internal Functions
    // ======================

    function _bondLicense(address operator, uint256 amount) internal {
        require(operator != address(0), ZeroAddress());
        require(amount != 0, ZeroAmount());

        address bondOwner = bondOwnerOf(operator);
        require(msg.sender == bondOwner, NotBondOwner(msg.sender, operator));

        operators[operator].licenseBond += amount;
        _bondedByOwner[bondOwner] += amount;

        uint256 balanceBefore = licenseToken.balanceOf(address(this));
        licenseToken.safeTransferFrom(msg.sender, address(this), amount);
        uint256 actualReceived = licenseToken.balanceOf(address(this)) -
            balanceBefore;
        require(actualReceived == amount, InvalidAmount());
        totalLicenseLiability += amount;

        emit LicenseBondUpdated(
            operator,
            int256(amount),
            operators[operator].licenseBond,
            REASON_BOND
        );

        _updateOperatorStatus(operator);
    }

    function _decreaseDelegatedBond(address operator, uint256 amount) internal {
        address bondOwner = bondOwnerOf(operator);
        _bondedByOwner[bondOwner] -= amount;
    }

    function _checkBondOwner(address operator) internal view {
        if (msg.sender != bondOwnerOf(operator)) {
            revert NotBondOwner(msg.sender, operator);
        }
    }

    /// @dev Updates operator's active status based on current conditions
    /// @dev Operator is active if: registered, has minimum license bond, and has minimum tickets
    /// @param operator Address of the operator to update
    function _updateOperatorStatus(address operator) internal {
        Operator storage op = operators[operator];
        uint256 currentVersion = eligibilityConfigurationVersion;
        bool oldActiveStatus = op.eligibilityVersion == currentVersion &&
            op.active;
        bool newActiveStatus = op.registered &&
            !_isOperatorBanned(operator) &&
            op.licenseBond >= _minLicenseBond() &&
            (ticketToken.balanceOf(operator) / ticketPrice >= minTicketBalance);

        op.eligibilityVersion = currentVersion;
        op.active = newActiveStatus;

        if (oldActiveStatus != newActiveStatus) {
            if (newActiveStatus) {
                numActiveOperators++;
            } else {
                numActiveOperators--;
            }

            emit OperatorActivationChanged(operator, newActiveStatus);
        }
    }

    /// @dev A ban from any retained slashing manager removes network eligibility.
    ///      A manager that cannot answer fails closed until governance repairs
    ///      or revokes that dependency.
    function _isOperatorBanned(address operator) internal view returns (bool) {
        uint256 len = _authorizedSlashingManagers.length;
        for (uint256 i = 0; i < len; i++) {
            (bool success, bytes memory result) = _authorizedSlashingManagers[i]
                .staticcall(
                    abi.encodeCall(ISlashingManager.isBanned, (operator))
                );
            if (!success || result.length != 32) return true;

            if (abi.decode(result, (uint256)) != 0) return true;
        }
        return false;
    }

    /// @dev Reads a lock-aware token through a checked low-level call so a bad
    ///      configuration returns a protocol error instead of an ABI decode error.
    function _lockedBalanceOf(
        IERC20 token,
        address account
    ) internal view returns (uint256) {
        (bool success, bytes memory result) = address(token).staticcall(
            abi.encodeCall(ILockAwareLicenseToken.lockedBalanceOf, (account))
        );
        if (!success || result.length != 32) {
            revert IncompatibleLicenseToken(address(token));
        }
        return abi.decode(result, (uint256));
    }

    /// @dev Calculates the minimum license bond required to maintain active status.
    /// @return Minimum license bond, rounded up to the next base unit.
    function _minLicenseBond() internal view returns (uint256) {
        return
            Math.mulDiv(
                licenseRequiredBond,
                licenseActiveBps,
                BPS_BASE,
                Math.Rounding.Ceil
            );
    }

    /// @dev Invalidates every cached active status in O(1). Operators are
    ///      considered inactive until they refresh under the new version.
    function _invalidateEligibilityStatuses() internal {
        eligibilityConfigurationVersion++;
        numActiveOperators = 0;
        emit EligibilityConfigurationVersionUpdated(
            eligibilityConfigurationVersion
        );
    }

    /// @dev `safeTransfer` of the license token, measuring the RECIPIENT-side delta
    ///      to detect fee-on-transfer / rebasing behavior (sender-side delta misses
    ///      fees that burn or reroute). Internal accounting is already decremented at
    ///      the call site, so a shortfall emits {LicenseTransferShortfall} rather than
    ///      reverting (a revert would brick claims if the token starts taking fees);
    ///      governance must pause new bonding and drain every liability before
    ///      rotating the token via {setLicenseToken}.
    function _safeTransferLicenseWithDeltaCheck(
        address recipient,
        uint256 expectedAmount
    ) internal {
        uint256 balanceBefore = licenseToken.balanceOf(recipient);
        licenseToken.safeTransfer(recipient, expectedAmount);
        uint256 balanceAfter = licenseToken.balanceOf(recipient);
        uint256 actualReceived = balanceAfter - balanceBefore;
        if (actualReceived != expectedAmount) {
            emit LicenseTransferShortfall(
                recipient,
                expectedAmount,
                actualReceived
            );
        }
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //              ERC-165 Interface Detection               //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice ERC-165 interface detection. Advertises
    ///         {IBondingRegistry} and {IERC165}.
    function supportsInterface(
        bytes4 interfaceId
    ) external pure virtual returns (bool) {
        return
            interfaceId == type(IBondingRegistry).interfaceId ||
            interfaceId == type(IERC165).interfaceId;
    }

    /// @dev Owner authorized by an operator. Zero means unset.
    mapping(address operator => address bondOwner) private _bondOwnerOf;

    /// @dev Aggregate license collateral owned by an account across operator keys.
    mapping(address bondOwner => uint256 amount) private _bondedByOwner;

    /// @dev Proposed owner in the two-step bond-owner transfer flow.
    mapping(address operator => address pendingOwner)
        private _pendingBondOwnerOf;

    /// @dev Reserved storage slots for future upgrades.
    // solhint-disable-next-line var-name-mixedcase
    uint256[44] private __gap;
}
