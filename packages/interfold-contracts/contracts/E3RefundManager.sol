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
    SafeERC20
} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {
    IERC165
} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import { IE3RefundManager } from "./interfaces/IE3RefundManager.sol";
import { IInterfold } from "./interfaces/IInterfold.sol";
import { IBondingRegistry } from "./interfaces/IBondingRegistry.sol";
import { ICiphernodeRegistry } from "./interfaces/ICiphernodeRegistry.sol";

/**
 * @title E3RefundManager
 * @notice Manages refund distribution for failed E3 computations
 * @dev Implements fault-attribution based refund system
 *
 */
// Per-E3 token, claim, routing, and immutable policy ledgers are intentionally
// isolated rather than packed into an opaque monolithic mapping.
// solhint-disable-next-line max-states-count
contract E3RefundManager is IE3RefundManager, Ownable2StepUpgradeable {
    using SafeERC20 for IERC20;
    ////////////////////////////////////////////////////////////
    //                                                        //
    //                 Storage Variables                      //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @notice The Interfold contract (contains lifecycle functionality)
    IInterfold public interfold;
    /// @notice The bonding registry for node rewards
    IBondingRegistry public bondingRegistry;
    /// @notice Protocol treasury for protocol fee collection
    address public treasury;
    /// @notice Work value allocation configuration
    WorkValueAllocation internal _workAllocation;
    /// @notice Maps E3 ID to refund distribution
    mapping(uint256 e3Id => RefundDistribution distribution)
        internal _distributions;
    /// @notice Tracks requester-refund claims per E3 per address
    mapping(uint256 e3Id => mapping(address claimer => bool hasClaimed))
        internal _requesterClaimed;
    /// @notice Tracks number of claims made per E3
    mapping(uint256 e3Id => uint256 count) internal _claimCount;
    /// @notice Tracks number of honest node claims made per E3
    mapping(uint256 e3Id => uint256 count) internal _honestNodeClaimCount;
    /// @notice Tracks total amount paid to honest nodes per E3
    mapping(uint256 e3Id => uint256 amount) internal _totalHonestNodePaid;
    /// @notice Maps E3 ID to honest node addresses
    mapping(uint256 e3Id => address[] nodes) internal _honestNodes;
    /// @notice Treasury pull-payment ledger for protocol slashed-fund share and dust.
    /// @dev Per-treasury so historical treasuries can drain even after rotation.
    mapping(address treasury => mapping(IERC20 token => uint256 amount))
        internal _pendingTreasury;
    /// @notice Tracks honest-node reward claims independently from requester claims
    mapping(uint256 e3Id => mapping(address claimer => bool hasClaimed))
        internal _honestNodeClaimed;
    /// @notice Per-E3 settlement economics frozen at request time.
    mapping(uint256 e3Id => E3PolicySnapshot snapshot)
        internal _e3PolicySnapshots;
    /// @notice Monotonic version for allocation or treasury changes.
    uint64 public policyVersion;
    /// @notice Unsettled slash escrow keyed by its actual underlying asset.
    mapping(uint256 e3Id => mapping(IERC20 token => uint256 amount))
        internal _pendingSlashedByToken;
    /// @notice Pull-payment slash entitlements, preserving token denomination.
    mapping(uint256 e3Id => mapping(IERC20 token => mapping(address account => uint256 amount)))
        internal _pendingSlashedClaims;
    /// @notice Slashed portion of the shared treasury pull ledger.
    mapping(address account => mapping(IERC20 token => uint256 amount))
        internal _pendingTrackedTreasury;
    /// @notice Protected slashed-fund liabilities for each token.
    mapping(IERC20 token => uint256 amount) internal _tokenLiability;
    /// @notice Whether successful completion enabled slash settlement for this E3.
    mapping(uint256 e3Id => bool ready) internal _successSettlementReady;
    ////////////////////////////////////////////////////////////
    //                                                        //
    //                       Modifiers                        //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @notice Restricts function to Interfold contract only
    modifier onlyInterfold() {
        if (msg.sender != address(interfold)) revert Unauthorized();
        _;
    }

    /// @dev Authorizes the Interfold contract frozen for an existing E3.
    modifier onlyE3Interfold(uint256 e3Id) {
        E3PolicySnapshot storage policy = _e3PolicySnapshots[e3Id];
        if (!policy.initialized || msg.sender != policy.interfold) {
            revert Unauthorized();
        }
        _;
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Initialization                       //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @notice Constructor that disables initializers
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the E3RefundManager contract
    /// @param _owner The owner address
    /// @param _interfold The Interfold contract address
    /// @param _treasury The protocol treasury address
    function initialize(
        address _owner,
        address _interfold,
        address _treasury
    ) public initializer {
        require(_owner != address(0), "Invalid owner");
        __Ownable_init(msg.sender);

        require(_interfold != address(0), "Invalid interfold");
        require(_treasury != address(0), "Invalid treasury");

        interfold = IInterfold(_interfold);
        bondingRegistry = interfold.bondingRegistry();
        treasury = _treasury;

        _workAllocation = WorkValueAllocation({
            committeeFormationBps: 1000,
            dkgBps: 3000,
            decryptionBps: 5500,
            protocolBps: 500,
            successSlashedNodeBps: 5000
        });
        policyVersion = 1;

        if (_owner != owner()) _transferOwnership(_owner);
    }

    /// @notice Maximum protocol share within {WorkValueAllocation}.
    uint16 public constant MAX_PROTOCOL_BPS = 5_000;

    /// @notice Basis-points denominator (100% = 10_000 bps).
    uint16 internal constant BPS_BASE = 10_000;

    /// @notice Thrown when {renounceOwnership} is called.
    error RenounceOwnershipDisabled();

    /// @notice Emitted whenever {interfold} is updated.
    event InterfoldUpdated(address indexed previous, address indexed next);

    /// @notice Emitted whenever {treasury} is updated.
    event TreasuryUpdated(address indexed previous, address indexed next);

    /// @notice Disabled. Reverts unconditionally.
    function renounceOwnership() public view override onlyOwner {
        revert RenounceOwnershipDisabled();
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //               Refund Calculation                       //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @inheritdoc IE3RefundManager
    function calculateRefund(
        uint256 e3Id,
        uint256 originalPayment,
        address[] calldata honestNodes,
        IERC20 paymentToken
    ) external onlyE3Interfold(e3Id) {
        require(!_distributions[e3Id].calculated, "Already calculated");
        require(originalPayment > 0, "No payment");
        require(address(paymentToken) != address(0), "Invalid fee token");

        // Attribute the failure before touching requester escrow. Supplier-side
        // failures return the entire fee payment; requester-side failures pay
        // completed work according to the request-time policy.
        IInterfold.FailureReason reason = _interfoldFor(e3Id).getFailureReason(
            e3Id
        );
        (
            uint256 honestNodeAmount,
            uint256 requesterAmount,
            uint256 protocolAmount
        ) = _baseDistribution(e3Id, originalPayment, reason);

        // No honest nodes: fold work share into the requester refund (mirrors
        // {Interfold._distributeRewards} success-path).
        if (honestNodes.length == 0 && honestNodeAmount > 0) {
            requesterAmount += honestNodeAmount;
            honestNodeAmount = 0;
        }

        // Store distribution. `perNodeAmount` is snapshotted below, AFTER any pending
        // slashed funds are folded in.
        _distributions[e3Id] = RefundDistribution({
            requesterAmount: requesterAmount,
            honestNodeAmount: honestNodeAmount,
            protocolAmount: protocolAmount,
            totalSlashed: 0,
            honestNodeCount: honestNodes.length,
            calculated: true,
            feeToken: paymentToken,
            originalPayment: originalPayment,
            perNodeAmount: 0
        });

        // Store honest nodes
        for (uint256 i = 0; i < honestNodes.length; i++) {
            _honestNodes[e3Id].push(honestNodes[i]);
        }

        // Credit protocol fee via pull-payment so a malicious/reverting/blacklisted
        // treasury cannot brick failed-E3 processing.
        if (protocolAmount > 0) {
            address policyTreasury = _treasuryFor(e3Id);
            _pendingTreasury[policyTreasury][paymentToken] += protocolAmount;
            emit TreasurySlashedCredited(
                policyTreasury,
                paymentToken,
                protocolAmount
            );
        }

        // Snapshot the base fee-token payout. Slashed assets are deliberately
        // separate token-specific pull claims and never mutate these amounts.
        RefundDistribution storage finalDist = _distributions[e3Id];
        if (honestNodes.length > 0) {
            finalDist.perNodeAmount =
                finalDist.honestNodeAmount /
                honestNodes.length;
        }

        if (_pendingSlashedByToken[e3Id][paymentToken] > 0) {
            _settleSlashedFunds(e3Id, paymentToken);
        }

        emit RefundDistributionCalculated(
            e3Id,
            finalDist.requesterAmount,
            finalDist.honestNodeAmount,
            finalDist.protocolAmount,
            finalDist.totalSlashed
        );
    }

    function _baseDistribution(
        uint256 e3Id,
        uint256 originalPayment,
        IInterfold.FailureReason reason
    )
        internal
        view
        returns (
            uint256 honestNodeAmount,
            uint256 requesterAmount,
            uint256 protocolAmount
        )
    {
        if (getFailurePayer(reason) == FailurePayer.Ciphernodes) {
            return (0, originalPayment, 0);
        }

        IInterfold.E3Stage failedAt = _getFailedAtStage(reason);
        (
            uint16 workCompletedBps,
            uint16 workRemainingBps
        ) = _calculateWorkValue(failedAt, _allocationFor(e3Id));
        honestNodeAmount = (originalPayment * workCompletedBps) / BPS_BASE;
        requesterAmount = (originalPayment * workRemainingBps) / BPS_BASE;
        protocolAmount = originalPayment - honestNodeAmount - requesterAmount;
    }

    /// @inheritdoc IE3RefundManager
    function getFailurePayer(
        IInterfold.FailureReason reason
    ) public pure returns (FailurePayer payer) {
        if (
            reason == IInterfold.FailureReason.NoInputsReceived ||
            reason == IInterfold.FailureReason.ComputeTimeout ||
            reason == IInterfold.FailureReason.ComputeProviderExpired ||
            reason == IInterfold.FailureReason.ComputeProviderFailed ||
            reason == IInterfold.FailureReason.RequesterCancelled
        ) {
            return FailurePayer.Requester;
        }

        if (
            reason == IInterfold.FailureReason.CommitteeFormationTimeout ||
            reason == IInterfold.FailureReason.InsufficientCommitteeMembers ||
            reason == IInterfold.FailureReason.DKGTimeout ||
            reason == IInterfold.FailureReason.DKGInvalidShares ||
            reason == IInterfold.FailureReason.DecryptionTimeout ||
            reason == IInterfold.FailureReason.DecryptionInvalidShares ||
            reason == IInterfold.FailureReason.VerificationFailed
        ) {
            return FailurePayer.Ciphernodes;
        }

        revert InvalidFailureReason(reason);
    }

    /// @notice Get the stage at which a requester-attributable E3 failed.
    function _getFailedAtStage(
        IInterfold.FailureReason reason
    ) internal pure returns (IInterfold.E3Stage) {
        // Map failure reason to stage
        if (
            reason == IInterfold.FailureReason.CommitteeFormationTimeout ||
            reason == IInterfold.FailureReason.InsufficientCommitteeMembers
        ) {
            return IInterfold.E3Stage.Requested;
        }
        if (
            reason == IInterfold.FailureReason.DKGTimeout ||
            reason == IInterfold.FailureReason.DKGInvalidShares
        ) {
            return IInterfold.E3Stage.CommitteeFinalized;
        }
        if (reason == IInterfold.FailureReason.NoInputsReceived) {
            return IInterfold.E3Stage.KeyPublished;
        }
        if (
            reason == IInterfold.FailureReason.ComputeTimeout ||
            reason == IInterfold.FailureReason.ComputeProviderExpired ||
            reason == IInterfold.FailureReason.ComputeProviderFailed ||
            reason == IInterfold.FailureReason.RequesterCancelled
        ) {
            return IInterfold.E3Stage.KeyPublished;
        }
        if (
            reason == IInterfold.FailureReason.DecryptionTimeout ||
            reason == IInterfold.FailureReason.DecryptionInvalidShares ||
            reason == IInterfold.FailureReason.VerificationFailed
        ) {
            return IInterfold.E3Stage.CiphertextReady;
        }

        return IInterfold.E3Stage.None;
    }

    /// @inheritdoc IE3RefundManager
    function calculateWorkValue(
        IInterfold.E3Stage stage
    ) public view returns (uint16 workCompletedBps, uint16 workRemainingBps) {
        return _calculateWorkValue(stage, _workAllocation);
    }

    function _calculateWorkValue(
        IInterfold.E3Stage stage,
        WorkValueAllocation memory alloc
    ) internal pure returns (uint16 workCompletedBps, uint16 workRemainingBps) {
        if (
            stage == IInterfold.E3Stage.Requested ||
            stage == IInterfold.E3Stage.None
        ) {
            // Failed at Requested = no work done
            workCompletedBps = 0;
        } else if (stage == IInterfold.E3Stage.CommitteeFinalized) {
            // Failed during DKG = sortition work done
            workCompletedBps = alloc.committeeFormationBps;
        } else if (stage == IInterfold.E3Stage.KeyPublished) {
            // Failed during input phase = sortition + DKG done (no additional work)
            workCompletedBps = alloc.committeeFormationBps + alloc.dkgBps;
        } else if (stage == IInterfold.E3Stage.CiphertextReady) {
            // Failed during decryption = sortition + DKG done (awaiting decryption shares)
            workCompletedBps = alloc.committeeFormationBps + alloc.dkgBps;
        }

        workRemainingBps = BPS_BASE - workCompletedBps - alloc.protocolBps;
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Claiming Functions                   //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @inheritdoc IE3RefundManager
    function claimRequesterRefund(
        uint256 e3Id
    ) external returns (uint256 amount) {
        RefundDistribution storage dist = _distributions[e3Id];
        if (!dist.calculated) revert RefundNotCalculated(e3Id);

        // A calculated distribution must always retain its settlement token.
        require(
            address(dist.feeToken) != address(0),
            "feeToken not initialized"
        );

        address requester = _interfoldFor(e3Id).getRequester(e3Id);
        if (msg.sender != requester) revert NotRequester(e3Id, msg.sender);

        if (_requesterClaimed[e3Id][msg.sender]) {
            revert AlreadyClaimed(e3Id, msg.sender);
        }

        amount = dist.requesterAmount;
        if (amount == 0) revert NoRefundAvailable(e3Id);

        _requesterClaimed[e3Id][msg.sender] = true;
        _claimCount[e3Id]++;

        // Use the per-E3 fee token (not the global one, which may have been rotated)
        _transferPreservingSlashedLiability(dist.feeToken, msg.sender, amount);

        emit RefundClaimed(e3Id, msg.sender, amount, "REQUESTER");
    }

    /// @inheritdoc IE3RefundManager
    function claimHonestNodeReward(
        uint256 e3Id
    ) external returns (uint256 amount) {
        RefundDistribution storage dist = _distributions[e3Id];
        require(dist.calculated, RefundNotCalculated(e3Id));

        // A calculated distribution must always retain its settlement token.
        require(
            address(dist.feeToken) != address(0),
            "feeToken not initialized"
        );

        require(
            !_honestNodeClaimed[e3Id][msg.sender],
            AlreadyClaimed(e3Id, msg.sender)
        );

        // Check if caller is honest node
        address[] memory nodes = _honestNodes[e3Id];
        bool isHonest = false;
        for (uint256 i = 0; i < nodes.length && !isHonest; i++) {
            isHonest = (nodes[i] == msg.sender);
        }
        require(isHonest, NotHonestNode(e3Id, msg.sender));

        require(dist.honestNodeCount > 0, NoRefundAvailable(e3Id));
        // Read the snapshot taken at `calculateRefund` time — immutable for the
        // distribution's lifetime; post-claim slashed funds remain in the
        // token-specific slash ledger and never mutate `dist.honestNodeAmount`.
        uint256 perNodeAmount = dist.perNodeAmount;
        require(perNodeAmount > 0, NoRefundAvailable(e3Id));

        amount = perNodeAmount;
        _honestNodeClaimCount[e3Id]++;
        if (_honestNodeClaimCount[e3Id] == dist.honestNodeCount) {
            // Route rounding dust to treasury via pull-payment so a reverting/blacklisted
            // treasury cannot brick the last honest claim. Computed from the snapshot so
            // the final claim is deterministic.
            uint256 paidIncludingThis = _totalHonestNodePaid[e3Id] +
                perNodeAmount;
            uint256 dust = dist.honestNodeAmount > paidIncludingThis
                ? dist.honestNodeAmount - paidIncludingThis
                : 0;
            if (dust > 0) {
                address policyTreasury = _treasuryFor(e3Id);
                _pendingTreasury[policyTreasury][dist.feeToken] += dust;
                emit TreasurySlashedCredited(
                    policyTreasury,
                    dist.feeToken,
                    dust
                );
            }
        }
        _totalHonestNodePaid[e3Id] += amount;

        _honestNodeClaimed[e3Id][msg.sender] = true;
        _claimCount[e3Id]++;

        // Direct transfer to the honest node (refund path; bypasses BondingRegistry
        // distributor authorization and operator-registered checks).
        IERC20 token = dist.feeToken;
        _transferPreservingSlashedLiability(token, msg.sender, amount);

        emit RefundClaimed(e3Id, msg.sender, amount, "HONEST_NODE");
    }

    /// @inheritdoc IE3RefundManager
    function escrowSlashedFunds(
        uint256 e3Id,
        IERC20 token,
        uint256 amount
    ) external onlyE3Interfold(e3Id) {
        require(amount > 0, "Zero amount");
        require(address(token) != address(0), "Invalid slash token");

        _pendingSlashedByToken[e3Id][token] += amount;
        _increaseTokenLiability(token, amount);

        if (_distributions[e3Id].calculated || _successSettlementReady[e3Id]) {
            _settleSlashedFunds(e3Id, token);
        }

        emit SlashedFundsEscrowed(e3Id, token, amount);
    }

    /// @inheritdoc IE3RefundManager
    function settleSlashedFunds(uint256 e3Id, IERC20 token) external {
        require(_pendingSlashedByToken[e3Id][token] > 0, NothingToClaim());
        require(
            _distributions[e3Id].calculated || _successSettlementReady[e3Id],
            "E3 not settled"
        );
        _settleSlashedFunds(e3Id, token);
    }

    /// @inheritdoc IE3RefundManager
    function distributeSlashedFundsOnSuccess(
        uint256 e3Id,
        IERC20 paymentToken
    ) external onlyE3Interfold(e3Id) {
        require(address(paymentToken) != address(0), "Invalid fee token");
        require(!_successSettlementReady[e3Id], "Already initialized");
        _successSettlementReady[e3Id] = true;

        if (_pendingSlashedByToken[e3Id][paymentToken] > 0) {
            _settleSlashedFunds(e3Id, paymentToken);
        }
    }

    function _settleSlashedFunds(uint256 e3Id, IERC20 token) internal {
        uint256 amount = _pendingSlashedByToken[e3Id][token];
        _pendingSlashedByToken[e3Id][token] = 0;
        (address[] memory nodes, ) = ICiphernodeRegistry(
            _e3PolicySnapshots[e3Id].registry
        ).getActiveCommitteeNodes(e3Id);

        if (_distributions[e3Id].calculated) {
            _settleFailedE3Slash(e3Id, token, amount, nodes);
        } else {
            _settleSuccessfulE3Slash(e3Id, token, amount, nodes);
        }
    }

    function _settleFailedE3Slash(
        uint256 e3Id,
        IERC20 token,
        uint256 amount,
        address[] memory nodes
    ) internal {
        RefundDistribution storage dist = _distributions[e3Id];
        uint256 toHonestNodes = amount;
        uint256 toProtocol;
        if (nodes.length == 0) {
            // There is no honest service-provider recipient. Do not turn a
            // punitive slash into requester over-compensation.
            toProtocol = amount;
            toHonestNodes = 0;
        }
        if (token == dist.feeToken) dist.totalSlashed += amount;

        _creditNodeSlashedClaims(e3Id, token, nodes, toHonestNodes);
        _creditTrackedTreasury(_treasuryFor(e3Id), token, toProtocol);
        emit SlashedFundsApplied(e3Id, token, 0, toHonestNodes);
    }

    function _settleSuccessfulE3Slash(
        uint256 e3Id,
        IERC20 token,
        uint256 amount,
        address[] memory nodes
    ) internal {
        uint256 toNodes = (amount * _successSlashedNodeBpsFor(e3Id)) / BPS_BASE;
        uint256 toProtocol = amount - toNodes;
        if (nodes.length == 0) {
            toProtocol += toNodes;
            toNodes = 0;
        }

        _creditNodeSlashedClaims(e3Id, token, nodes, toNodes);
        _creditTrackedTreasury(_treasuryFor(e3Id), token, toProtocol);
        emit SlashedFundsDistributedOnSuccess(e3Id, token, toNodes, toProtocol);
    }

    function _creditNodeSlashedClaims(
        uint256 e3Id,
        IERC20 token,
        address[] memory nodes,
        uint256 amount
    ) internal {
        if (amount == 0) return;
        uint256 perNode = amount / nodes.length;
        uint256 distributed;
        for (uint256 i = 0; i < nodes.length; i++) {
            uint256 nodeAmount = i == nodes.length - 1
                ? amount - distributed
                : perNode;
            _creditSlashedClaim(e3Id, token, nodes[i], nodeAmount);
            distributed += nodeAmount;
        }
    }

    function _creditSlashedClaim(
        uint256 e3Id,
        IERC20 token,
        address account,
        uint256 amount
    ) internal {
        if (amount == 0) return;
        _pendingSlashedClaims[e3Id][token][account] += amount;
        emit SlashedFundsCredited(e3Id, account, token, amount);
    }

    function _creditTrackedTreasury(
        address account,
        IERC20 token,
        uint256 amount
    ) internal {
        if (amount == 0) return;
        _pendingTreasury[account][token] += amount;
        _pendingTrackedTreasury[account][token] += amount;
        emit TreasurySlashedCredited(account, token, amount);
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                    View Functions                      //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @inheritdoc IE3RefundManager
    function getRefundDistribution(
        uint256 e3Id
    ) external view returns (RefundDistribution memory) {
        return _distributions[e3Id];
    }

    /// @inheritdoc IE3RefundManager
    function pendingSlashedFunds(
        uint256 e3Id,
        IERC20 token
    ) external view returns (uint256) {
        return _pendingSlashedByToken[e3Id][token];
    }

    /// @inheritdoc IE3RefundManager
    function pendingSlashedClaim(
        uint256 e3Id,
        IERC20 token,
        address account
    ) external view returns (uint256) {
        return _pendingSlashedClaims[e3Id][token][account];
    }

    /// @inheritdoc IE3RefundManager
    function tokenLiability(IERC20 token) external view returns (uint256) {
        return _tokenLiability[token];
    }

    /// @inheritdoc IE3RefundManager
    function hasRequesterClaimed(
        uint256 e3Id,
        address claimant
    ) external view returns (bool) {
        return _requesterClaimed[e3Id][claimant];
    }

    /// @inheritdoc IE3RefundManager
    function hasHonestNodeClaimed(
        uint256 e3Id,
        address claimant
    ) external view returns (bool) {
        return _honestNodeClaimed[e3Id][claimant];
    }

    /// @inheritdoc IE3RefundManager
    function getWorkAllocation()
        external
        view
        returns (WorkValueAllocation memory)
    {
        return _workAllocation;
    }

    /// @inheritdoc IE3RefundManager
    function getE3PolicySnapshot(
        uint256 e3Id
    ) external view returns (E3PolicySnapshot memory snapshot) {
        return _e3PolicySnapshots[e3Id];
    }

    /// @inheritdoc IE3RefundManager
    function snapshotE3Policy(
        uint256 e3Id,
        address registry
    ) external onlyInterfold {
        E3PolicySnapshot storage snapshot = _e3PolicySnapshots[e3Id];
        require(!snapshot.initialized, "Policy already snapshotted");
        require(registry != address(0), "Invalid registry");
        if (policyVersion == 0) policyVersion = 1;
        snapshot.allocation = _workAllocation;
        snapshot.treasury = treasury;
        snapshot.interfold = msg.sender;
        snapshot.registry = registry;
        snapshot.version = policyVersion;
        snapshot.initialized = true;
        emit E3PolicySnapshotted(
            e3Id,
            policyVersion,
            treasury,
            msg.sender,
            registry,
            _workAllocation
        );
    }

    function _treasuryFor(uint256 e3Id) internal view returns (address) {
        E3PolicySnapshot storage policy = _e3PolicySnapshots[e3Id];
        return policy.treasury;
    }

    function _interfoldFor(uint256 e3Id) internal view returns (IInterfold) {
        E3PolicySnapshot storage policy = _e3PolicySnapshots[e3Id];
        return IInterfold(policy.interfold);
    }

    function _allocationFor(
        uint256 e3Id
    ) internal view returns (WorkValueAllocation memory allocation) {
        E3PolicySnapshot storage policy = _e3PolicySnapshots[e3Id];
        return policy.allocation;
    }

    function _successSlashedNodeBpsFor(
        uint256 e3Id
    ) internal view returns (uint16) {
        E3PolicySnapshot storage policy = _e3PolicySnapshots[e3Id];
        return policy.allocation.successSlashedNodeBps;
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Admin Functions                      //
    //                                                        //
    ////////////////////////////////////////////////////////////
    /// @inheritdoc IE3RefundManager
    function setWorkAllocation(
        WorkValueAllocation calldata allocation
    ) external onlyOwner {
        // cap protocol BPS at 50% so a malicious owner cannot route
        // an arbitrary share of fees to the protocol treasury.
        require(
            allocation.protocolBps <= MAX_PROTOCOL_BPS,
            "Protocol BPS too high"
        );
        uint256 total = uint256(allocation.committeeFormationBps) +
            uint256(allocation.dkgBps) +
            uint256(allocation.decryptionBps) +
            uint256(allocation.protocolBps);
        require(total == BPS_BASE, "Must sum to 10000");
        require(allocation.successSlashedNodeBps <= BPS_BASE, "Invalid BPS");

        _workAllocation = allocation;
        policyVersion = policyVersion == 0 ? 1 : policyVersion + 1;

        emit WorkAllocationUpdated(allocation);
    }

    /// @notice Set the Interfold contract address
    /// @param _interfold New Interfold address
    function setInterfold(address _interfold) external onlyOwner {
        require(_interfold != address(0), "Invalid interfold");
        address oldValue = address(interfold);
        interfold = IInterfold(_interfold);
        emit InterfoldUpdated(oldValue, _interfold);
    }

    /// @notice Set the treasury address
    /// @param _treasury New treasury address
    function setTreasury(address _treasury) external onlyOwner {
        require(_treasury != address(0), "Invalid treasury");
        address oldValue = treasury;
        treasury = _treasury;
        policyVersion = policyVersion == 0 ? 1 : policyVersion + 1;
        emit TreasuryUpdated(oldValue, _treasury);
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //              Pull-Payment Claim Functions              //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @inheritdoc IE3RefundManager
    function claimSlashedFunds(
        uint256 e3Id,
        IERC20 token
    ) external returns (uint256 amount) {
        amount = _pendingSlashedClaims[e3Id][token][msg.sender];
        require(amount > 0, NothingToClaim());
        _pendingSlashedClaims[e3Id][token][msg.sender] = 0;
        _decreaseTokenLiability(token, amount);
        _transferPreservingSlashedLiability(token, msg.sender, amount);
        emit SlashedFundsClaimed(e3Id, msg.sender, token, amount);
    }

    /// @inheritdoc IE3RefundManager
    function treasuryClaim(IERC20 token) external returns (uint256 amount) {
        amount = _pendingTreasury[msg.sender][token];
        require(amount > 0, NothingToClaim());
        _pendingTreasury[msg.sender][token] = 0;
        uint256 tracked = _pendingTrackedTreasury[msg.sender][token];
        if (tracked > 0) {
            _pendingTrackedTreasury[msg.sender][token] = 0;
            _decreaseTokenLiability(token, tracked);
        }
        _transferPreservingSlashedLiability(token, msg.sender, amount);
        emit TreasurySlashedClaimed(msg.sender, token, amount);
    }

    /// @inheritdoc IE3RefundManager
    function pendingTreasuryClaim(
        address treasuryAddr,
        IERC20 token
    ) external view returns (uint256) {
        return _pendingTreasury[treasuryAddr][token];
    }

    function _increaseTokenLiability(IERC20 token, uint256 amount) internal {
        _tokenLiability[token] += amount;
        _assertTokenSolvent(token);
    }

    function _decreaseTokenLiability(IERC20 token, uint256 amount) internal {
        _tokenLiability[token] -= amount;
    }

    function _transferPreservingSlashedLiability(
        IERC20 token,
        address recipient,
        uint256 amount
    ) internal {
        token.safeTransfer(recipient, amount);
        _assertTokenSolvent(token);
    }

    function _assertTokenSolvent(IERC20 token) internal view {
        uint256 liability = _tokenLiability[token];
        uint256 balance = token.balanceOf(address(this));
        if (balance < liability) {
            revert InsolventToken(token, liability, balance);
        }
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //              ERC-165 Interface Detection               //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice ERC-165 interface detection. Advertises
    ///         {IE3RefundManager} and {IERC165}.
    function supportsInterface(
        bytes4 interfaceId
    ) external pure virtual returns (bool) {
        return
            interfaceId == type(IE3RefundManager).interfaceId ||
            interfaceId == type(IERC165).interfaceId;
    }

    /// @dev Reserved storage slots for future upgrades.
    // solhint-disable-next-line var-name-mixedcase
    uint256[50] private __gap;
}
