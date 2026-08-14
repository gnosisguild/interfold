// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { ICiphernodeRegistry } from "../interfaces/ICiphernodeRegistry.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { E3 } from "../interfaces/IE3.sol";
import { IInterfold } from "../interfaces/IInterfold.sol";
import { IPkVerifier } from "../interfaces/IPkVerifier.sol";
import { ISlashingManager } from "../interfaces/ISlashingManager.sol";
import {
    Ownable2StepUpgradeable
} from "@openzeppelin/contracts-upgradeable/access/Ownable2StepUpgradeable.sol";
import {
    InternalLazyIMT,
    LazyIMTData
} from "@zk-kit/lazy-imt.sol/InternalLazyIMT.sol";
import {
    IERC165
} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import { CommitteeHashLib } from "../lib/CommitteeHashLib.sol";
import { RegistrySortitionLib } from "../lib/RegistrySortitionLib.sol";
import {
    IDkgFoldAttestationVerifier
} from "../interfaces/IDkgFoldAttestationVerifier.sol";

/**
 * @title CiphernodeRegistryOwnable
 * @notice Ownable implementation of the ciphernode registry with IMT-based membership tracking
 * @dev Manages ciphernode registration, committee selection, and integrates with bonding registry
 */
// solhint-disable-next-line max-states-count
contract CiphernodeRegistryOwnable is
    ICiphernodeRegistry,
    Ownable2StepUpgradeable
{
    using InternalLazyIMT for LazyIMTData;

    /// @notice Thrown when {renounceOwnership} is called.
    error RenounceOwnershipDisabled();

    /// @notice Minimum permitted value for {sortitionSubmissionWindow}.
    uint256 public constant MIN_SORTITION_SUBMISSION_WINDOW = 60;

    /// @notice Maximum permitted value for {sortitionSubmissionWindow}.
    /// @dev One day fits inside Arbitrum's approximately 27-hour L2 hash history.
    uint256 public constant MAX_SORTITION_SUBMISSION_WINDOW = 1 days;

    /// @notice EIP-2935 block-hash history contract.
    address public constant BLOCKHASH_HISTORY =
        0x0000F90827F1C53a10cb7A02335B175320002935;

    /// @notice Thrown when {setSortitionSubmissionWindow} input is outside the
    ///         permitted window.
    error SortitionSubmissionWindowOutOfBounds(uint256 window);

    /// @notice Emitted whenever {slashingManager} is updated.
    event RegistrySlashingManagerSet(address indexed slashingManager);

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                        Events                          //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Emitted when the bonding registry address is set
    /// @param bondingRegistry Address of the bonding registry contract
    event BondingRegistrySet(address indexed bondingRegistry);

    /// @notice Emitted when the slashing manager address is set
    /// @param slashingManager Address of the slashing manager contract
    event SlashingManagerSet(address indexed slashingManager);

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                 Storage Variables                      //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Address of the Interfold contract authorized to request committees
    IInterfold public interfold;

    /// @notice Address of the bonding registry for checking node eligibility
    IBondingRegistry public bondingRegistry;

    /// @notice Current number of registered ciphernodes
    uint256 public override numCiphernodes;

    /// @notice Submission Window for an E3 Sortition.
    /// @dev The submission window is the time period during which the ciphernodes can submit
    /// their tickets to be a part of the committee.
    uint256 public sortitionSubmissionWindow;

    /// @notice Depth of the LazyIMT tree
    uint8 public constant TREE_DEPTH = 20;

    /// @notice Maximum number of leaves the underlying LazyIMT can hold.
    /// @dev New slots cannot be allocated after the tree reaches this cap. Removed
    ///      slots are reused before the registry allocates another leaf.
    uint256 public constant MAX_CIPHERNODE_LEAVES = uint256(1) << TREE_DEPTH;

    /// @notice Lifetime insertion count at which operators must prepare a new tree generation.
    uint256 public constant CIPHERNODE_TREE_WARNING_THRESHOLD =
        (MAX_CIPHERNODE_LEAVES * 4) / 5;

    /// @notice Maximum serialized public-key candidate size.
    /// @dev Covers every currently supported BFV preset, including SecureThreshold8192.
    uint256 public constant MAX_COMMITTEE_PUBLIC_KEY_BYTES = 256 * 1024;

    /// @notice Thrown when {addCiphernode} would push the LazyIMT past its
    ///         configured {TREE_DEPTH} capacity.
    error CiphernodeTreeExhausted();

    /// @notice Incremental Merkle Tree (IMT) containing all registered ciphernodes
    LazyIMTData public ciphernodes;

    /// @notice Tracks whether a ciphernode is enabled in the registry
    mapping(address node => bool enabled) public ciphernodeEnabled;

    /// @notice Tracks the tree leaf index for each ciphernode
    mapping(address node => uint40 index) public ciphernodeTreeIndex;

    /// @notice Maps E3 ID to the IMT root at the time of committee request
    mapping(uint256 e3Id => uint256 root) public roots;

    /// @notice Maps E3 ID to the hash of the committee's public key
    mapping(uint256 e3Id => bytes32 publicKeyHash) public publicKeyHashes;

    /// @notice Maps E3 ID to its committee data
    mapping(uint256 e3Id => Committee committee) internal committees;

    /// @notice Address of the slashing manager authorized to expel committee members
    ISlashingManager public slashingManager;

    /// @notice Verifies per-node DKG fold attestations at publication (external contract).
    IDkgFoldAttestationVerifier public dkgFoldAttestationVerifier;

    /// @notice Minimum delay between proposing a verifier change and committing it.
    /// @dev Treats `dkgFoldAttestationVerifier` as a critical admin key: a compromised
    ///      owner cannot instantly swap it for a weak verifier that bypasses
    ///      per-party attestation checks; the proposal is visible on-chain for
    ///      this window and can be cancelled by the (recovered) legitimate owner.
    uint256 public constant DKG_FOLD_VERIFIER_TIMELOCK = 2 days;

    /// @notice Pending verifier proposal awaiting commit. `pendingAt == 0` means no proposal.
    address public pendingDkgFoldAttestationVerifier;
    uint256 public pendingDkgFoldAttestationVerifierAt;

    /// @notice Registry-wide validity window (seconds) for accusation vote deadlines.
    ///         Ciphernodes use it to create deadlines and validate peer accusations.
    ///         The slashing manager snapshots the value for each E3 and bounds the
    ///         signed issue time, validity window, and objective submission deadline.
    ///
    /// @dev Set with [`setAccusationVoteValidity`] by `owner()`. Defaults to the
    ///      [`DEFAULT_ACCUSATION_VOTE_VALIDITY`] constant on initialize so newly-
    ///      deployed registries are operational without an extra setter call.
    ///      Setting the live value to zero makes the slashing manager reject new
    ///      attestation-based proposals. Governance can use this as an emergency stop.
    uint256 public accusationVoteValidity;

    /// @notice Default value for `accusationVoteValidity` applied at `initialize`.
    /// @dev 30 minutes covers gossip latency, vote-collection timeout, and mempool
    ///      congestion while keeping stolen signatures from being replayed indefinitely.
    uint256 public constant DEFAULT_ACCUSATION_VOTE_VALIDITY = 30 minutes;

    /// @notice Minimum delay between proposing and committing a zeroing vote-validity update.
    /// @dev Mirrors verifier critical-change timelock posture for slash-disable behavior.
    uint256 public constant ACCUSATION_VOTE_VALIDITY_TIMELOCK = 2 days;

    /// @notice Pending vote-validity proposal awaiting commit. `pendingAt == 0` means none.
    uint256 public pendingAccusationVoteValidity;
    uint256 public pendingAccusationVoteValidityAt;

    /// @notice DKG anchor commitments stored when the committee public key is published.
    mapping(uint256 e3Id => uint256[] partyIds) internal dkgPartyIds;
    mapping(uint256 e3Id => bytes32[] skAggCommits) internal dkgSkAggCommits;
    mapping(uint256 e3Id => bytes32[] esmAggCommits) internal dkgEsmAggCommits;

    struct CommitteeDependencies {
        IInterfold interfoldContract;
        IBondingRegistry bonding;
        ISlashingManager slashManager;
        IDkgFoldAttestationVerifier dkgFoldAttestationVerifier;
    }

    /// @notice External contracts frozen for each committee lifecycle.
    mapping(uint256 e3Id => CommitteeDependencies dependencies)
        internal _committeeDependencies;

    /// @notice Ticket price frozen for each E3 sortition.
    mapping(uint256 e3Id => uint256 ticketPrice) public sortitionTicketPrices;

    /// @dev Highest committee deadline created by a request.
    uint256 private _latestCommitteeDeadline;

    /// @notice Future block committed when an E3 requests a committee.
    mapping(uint256 e3Id => uint256 blockNumber) public sortitionEntropyBlocks;

    /// @notice Whether the committee seed has been stored for an E3.
    mapping(uint256 e3Id => bool resolved) public sortitionSeedResolved;

    /// @inheritdoc ICiphernodeRegistry
    uint256 public unreleasedCommitteeCount;

    /// @dev Removed tree slots that can be assigned to future registrations.
    uint40[] private _freeCiphernodeTreeIndices;

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                     Modifiers                          //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @dev Restricts function access to only the Interfold contract
    modifier onlyInterfold() {
        require(msg.sender == address(interfold), OnlyInterfold());
        _;
    }

    /// @dev Restricts function access to owner or bonding registry
    modifier onlyOwnerOrBondingVault() {
        require(
            msg.sender == owner() || msg.sender == address(bondingRegistry),
            NotOwnerOrBondingRegistry()
        );
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

    /// @notice Initializes the registry contract
    /// @param _owner Address that will own the contract
    /// @param _submissionWindow The submission window for the E3 sortition in seconds
    function initialize(
        address _owner,
        uint256 _submissionWindow
    ) public initializer {
        require(_owner != address(0), ZeroAddress());

        // Hold ownership transiently as `msg.sender` so the internal call to
        // `setSortitionSubmissionWindow` (which is `onlyOwner`) succeeds, then
        // transfer to the final `_owner` before returning.
        __Ownable_init(msg.sender);
        ciphernodes._init(TREE_DEPTH);
        setSortitionSubmissionWindow(_submissionWindow);
        // Seed the off-chain freshness window with a sensible default so new
        // deployments don't immediately need a governance call before slashing
        // becomes operational.
        accusationVoteValidity = DEFAULT_ACCUSATION_VOTE_VALIDITY;
        emit AccusationVoteValiditySet(DEFAULT_ACCUSATION_VOTE_VALIDITY);
        if (_owner != owner()) _transferOwnership(_owner);
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                  Core Entrypoints                      //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @inheritdoc ICiphernodeRegistry
    /// @dev The eligible count uses the same `requestBlock - 1` boundary as ticket submission.
    ///      Submission also requires current activity as a liveness check.
    function requestCommittee(
        uint256 e3Id,
        uint256,
        uint32[2] calldata threshold
    ) external onlyInterfold returns (bool success) {
        Committee storage c = committees[e3Id];
        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.None,
            CommitteeAlreadyRequested()
        );

        CommitteeDependencies storage dependencies = _committeeDependencies[
            e3Id
        ];
        dependencies.interfoldContract = IInterfold(msg.sender);
        dependencies.bonding = bondingRegistry;
        dependencies.slashManager = slashingManager;
        require(
            address(dkgFoldAttestationVerifier) != address(0),
            FoldAttestationVerifierNotSet()
        );
        dependencies.dkgFoldAttestationVerifier = dkgFoldAttestationVerifier;
        dependencies.slashManager.snapshotE3Dependencies(e3Id);
        dependencies.bonding.setCommitteeObligation(e3Id, address(0), true);
        unreleasedCommitteeCount++;
        emit DkgFoldAttestationContextEstablished(
            e3Id,
            address(this),
            address(dependencies.dkgFoldAttestationVerifier)
        );

        c.requestBlock = block.timestamp;
        (, uint256 activeCount) = dependencies.bonding.eligibilityAt(
            address(0),
            c.requestBlock - 1
        );
        require(
            threshold[1] <= activeCount,
            InsufficientCiphernodes(threshold[1], activeCount)
        );

        uint256 ticketPrice = dependencies.bonding.ticketPrice();
        require(ticketPrice > 0, InvalidTicketNumber());
        sortitionTicketPrices[e3Id] = ticketPrice;

        c.stage = ICiphernodeRegistry.CommitteeStage.Requested;
        uint256 entropyBlock = RegistrySortitionLib.currentBlockNumber(
            block.chainid
        ) + 1;
        sortitionEntropyBlocks[e3Id] = entropyBlock;
        // NOTE: `requestBlock` stores a timepoint per EIP-6372 (mode=timestamp) — its name
        // is kept for storage/event compatibility but it must be compared to
        // {block.timestamp}. This matches the InterfoldTicketToken's timestamp-mode clock so
        // {getPastVotes} lookups resolve consistently.
        c.committeeDeadline = block.timestamp + sortitionSubmissionWindow;
        if (c.committeeDeadline > _latestCommitteeDeadline) {
            _latestCommitteeDeadline = c.committeeDeadline;
        }
        c.threshold = threshold;
        roots[e3Id] = root();

        emit CommitteeRequested(
            e3Id,
            entropyBlock,
            threshold,
            c.requestBlock,
            c.committeeDeadline,
            ticketPrice
        );
        success = true;
    }

    /// @inheritdoc ICiphernodeRegistry
    function publishCommittee(
        uint256 e3Id,
        bytes32 pkCommitment,
        bytes calldata proof,
        bytes calldata dkgAttestationBundle
    ) external {
        Committee storage c = committees[e3Id];

        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.Finalized,
            CommitteeNotFinalized()
        );
        require(c.activeCount >= c.threshold[0], ThresholdNotMet());
        require(c.publicKey == bytes32(0), CommitteeAlreadyPublished());
        require(pkCommitment != bytes32(0), PkCommitmentRequired());

        bytes32 committeeHash = CommitteeHashLib.hash(c.topNodes);
        c.committeeHash = committeeHash;
        c.publicKey = pkCommitment;
        publicKeyHashes[e3Id] = pkCommitment;

        E3 memory e3 = _interfoldFor(e3Id).getE3(e3Id);
        // Bind to the on-chain committee (c.topNodes), not caller-supplied
        // nodes, so a wrong `nodes` input cannot pre-commit the prover to
        // an attacker's set (C-08).
        _verifyAndStoreDkgAnchors(
            e3Id,
            e3,
            roots[e3Id],
            c.topNodes,
            pkCommitment,
            committeeHash,
            proof,
            dkgAttestationBundle
        );

        _interfoldFor(e3Id).onCommitteePublished(e3Id, pkCommitment);

        emit CommitteeProofPublished(e3Id, c.topNodes, pkCommitment, proof);
    }

    /// @inheritdoc ICiphernodeRegistry
    function publishCommitteePublicKey(
        uint256 e3Id,
        bytes calldata publicKey
    ) external {
        bytes32 pkCommitment = publicKeyHashes[e3Id];
        require(pkCommitment != bytes32(0), CommitteeNotPublished());

        uint256 length = publicKey.length;
        require(
            length != 0 && length <= MAX_COMMITTEE_PUBLIC_KEY_BYTES,
            InvalidPublicKeyLength(length, MAX_COMMITTEE_PUBLIC_KEY_BYTES)
        );

        emit CommitteePublished(
            e3Id,
            committees[e3Id].topNodes,
            publicKey,
            pkCommitment,
            bytes("")
        );
    }

    function _verifyAndStoreDkgAnchors(
        uint256 e3Id,
        E3 memory e3,
        uint256 committeeRoot,
        address[] memory sortedNodes,
        bytes32 pkCommitment,
        bytes32 committeeHash,
        bytes calldata proof,
        bytes calldata dkgAttestationBundle
    ) internal {
        require(proof.length > 0, DkgProofRequired());
        // Reverts with a typed error on any mismatch; binds to the on-chain
        // committee (sortedNodes = c.topNodes) per audit finding C-08.
        if (
            !e3.pkVerifier.verify(
                e3Id,
                committeeRoot,
                sortedNodes,
                pkCommitment,
                committeeHash,
                proof
            )
        ) revert IPkVerifier.InvalidProof();
        _verifyAndStoreFoldAttestation(e3Id, proof, dkgAttestationBundle);
    }

    /// @dev Split out to avoid "stack too deep" in `_verifyAndStoreDkgAnchors`.
    function _verifyAndStoreFoldAttestation(
        uint256 e3Id,
        bytes calldata proof,
        bytes calldata dkgAttestationBundle
    ) internal {
        require(dkgAttestationBundle.length > 0, FoldAttestationsRequired());
        IDkgFoldAttestationVerifier verifier = _dkgFoldAttestationVerifierFor(
            e3Id
        );
        require(
            address(verifier) != address(0),
            FoldAttestationVerifierNotSet()
        );

        (
            uint256[] memory partyIds,
            bytes32[] memory skAgg,
            bytes32[] memory esmAgg
        ) = verifier.verify(
                address(this),
                block.chainid,
                e3Id,
                proof,
                dkgAttestationBundle
            );

        dkgPartyIds[e3Id] = partyIds;
        dkgSkAggCommits[e3Id] = skAgg;
        dkgEsmAggCommits[e3Id] = esmAgg;
    }

    /// @notice Propose a new DKG fold-attestation verifier. The change becomes active
    ///         only after `DKG_FOLD_VERIFIER_TIMELOCK` has elapsed and `commitDkgFoldAttestationVerifier`
    ///         is called. Replaces any pending proposal.
    /// @dev First-time set is also subject to the timelock — operators must wait
    ///      the same window before the verifier is active. For the deploy-time
    ///      initial set, see `setInitialDkgFoldAttestationVerifier`.
    ///
    /// @dev Each committee keeps the verifier that was active when it was requested.
    ///      The context-established event tells ciphernodes which verifier to use.
    function proposeDkgFoldAttestationVerifier(
        IDkgFoldAttestationVerifier verifier
    ) external onlyOwner {
        require(address(verifier) != address(0), ZeroAddress());
        pendingDkgFoldAttestationVerifier = address(verifier);
        pendingDkgFoldAttestationVerifierAt = block.timestamp;
        emit DkgFoldAttestationVerifierProposed(
            address(verifier),
            block.timestamp + DKG_FOLD_VERIFIER_TIMELOCK
        );
    }

    /// @notice Commit a previously proposed verifier change after the timelock elapses.
    /// @param verifier Must match the pending proposal (prevents commit-time substitution).
    function commitDkgFoldAttestationVerifier(
        IDkgFoldAttestationVerifier verifier
    ) external onlyOwner {
        address pending = pendingDkgFoldAttestationVerifier;
        require(pending != address(0), NoPendingVerifierUpdate());
        require(
            pending == address(verifier),
            VerifierMismatch(pending, address(verifier))
        );
        uint256 readyAt = pendingDkgFoldAttestationVerifierAt +
            DKG_FOLD_VERIFIER_TIMELOCK;
        require(
            block.timestamp >= readyAt,
            VerifierUpdateTimelockActive(readyAt, block.timestamp)
        );
        dkgFoldAttestationVerifier = verifier;
        pendingDkgFoldAttestationVerifier = address(0);
        pendingDkgFoldAttestationVerifierAt = 0;
        emit DkgFoldAttestationVerifierUpdated(address(verifier));
    }

    /// @notice Cancel a pending verifier proposal.
    function cancelDkgFoldAttestationVerifierProposal() external onlyOwner {
        address pending = pendingDkgFoldAttestationVerifier;
        require(pending != address(0), NoPendingVerifierUpdate());
        pendingDkgFoldAttestationVerifier = address(0);
        pendingDkgFoldAttestationVerifierAt = 0;
        emit DkgFoldAttestationVerifierProposalCancelled(pending);
    }

    /// @notice One-shot initial set, allowed only when no verifier has ever been configured.
    /// @dev Lets deploy scripts wire the verifier without first waiting the timelock.
    ///      Subsequent changes must go through `propose`/`commit`. Cannot be used to
    ///      bypass the timelock for replacement — only for the very first set.
    function setInitialDkgFoldAttestationVerifier(
        IDkgFoldAttestationVerifier verifier
    ) external onlyOwner {
        require(
            address(dkgFoldAttestationVerifier) == address(0),
            FoldAttestationVerifierAlreadySet()
        );
        require(address(verifier) != address(0), ZeroAddress());
        dkgFoldAttestationVerifier = verifier;
        // Invalidate any stale pending proposal made before the initial set,
        // so it cannot later be committed and silently bypass the timelock.
        if (pendingDkgFoldAttestationVerifier != address(0)) {
            address staleProposal = pendingDkgFoldAttestationVerifier;
            pendingDkgFoldAttestationVerifier = address(0);
            pendingDkgFoldAttestationVerifierAt = 0;
            emit DkgFoldAttestationVerifierProposalCancelled(staleProposal);
        }
        emit DkgFoldAttestationVerifierUpdated(address(verifier));
    }

    /// @inheritdoc ICiphernodeRegistry
    function addCiphernode(address node) external onlyOwnerOrBondingVault {
        if (isEnabled(node)) {
            return;
        }

        uint40 index;
        uint256 freeCount = _freeCiphernodeTreeIndices.length;
        if (freeCount == 0) {
            index = ciphernodes.numberOfLeaves;
            require(
                uint256(index) < MAX_CIPHERNODE_LEAVES,
                CiphernodeTreeExhausted()
            );
            ciphernodes._insert(uint160(node));
            if (
                ciphernodes.numberOfLeaves == CIPHERNODE_TREE_WARNING_THRESHOLD
            ) {
                emit CiphernodeTreeCapacityWarning(
                    ciphernodes.numberOfLeaves,
                    MAX_CIPHERNODE_LEAVES
                );
            }
        } else {
            index = _freeCiphernodeTreeIndices[freeCount - 1];
            _freeCiphernodeTreeIndices.pop();
            ciphernodes._update(uint160(node), index);
        }
        ciphernodeEnabled[node] = true;
        ciphernodeTreeIndex[node] = index;
        numCiphernodes++;
        emit CiphernodeAdded(
            node,
            index,
            numCiphernodes,
            ciphernodes.numberOfLeaves
        );
    }

    /// @inheritdoc ICiphernodeRegistry
    function removeCiphernode(address node) external onlyOwnerOrBondingVault {
        require(isEnabled(node), CiphernodeNotEnabled(node));

        uint40 index = ciphernodeTreeIndex[node];
        ciphernodes._update(0, index);
        _freeCiphernodeTreeIndices.push(index);
        ciphernodeEnabled[node] = false;
        numCiphernodes--;
        emit CiphernodeRemoved(
            node,
            index,
            numCiphernodes,
            ciphernodes.numberOfLeaves
        );
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Sortition Functions                  //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Submit a ticket for sortition
    /// @dev Validates the ticket against request-boundary state and inserts it into the top-N.
    /// @param e3Id ID of the E3 computation
    /// @param ticketNumber The ticket number to submit (1 to available tickets at snapshot)
    function submitTicket(uint256 e3Id, uint256 ticketNumber) external {
        Committee storage c = committees[e3Id];
        require(
            c.stage != ICiphernodeRegistry.CommitteeStage.None,
            CommitteeNotRequested()
        );
        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.Requested,
            CommitteeAlreadyFinalized()
        );
        require(
            block.timestamp <= c.committeeDeadline,
            CommitteeDeadlineReached()
        );
        require(!c.submitted[msg.sender], NodeAlreadySubmitted());
        (bool activeAtRequest, ) = _bondingFor(e3Id).eligibilityAt(
            msg.sender,
            c.requestBlock - 1
        );
        require(
            isEnabled(msg.sender) &&
                _bondingFor(e3Id).isActive(msg.sender) &&
                activeAtRequest,
            NodeNotEligible()
        );

        // Validate node eligibility and ticket number
        _validateNodeEligibility(msg.sender, ticketNumber, e3Id);

        uint256 seed = _resolveSortitionSeed(e3Id, c);

        // The ticket snapshot predates the request, while the seed comes from
        // a block that is committed only after the request succeeds.
        uint256 score = _computeTicketScore(
            msg.sender,
            ticketNumber,
            e3Id,
            seed
        );

        // Store submission
        c.submitted[msg.sender] = true;

        RegistrySortitionLib.insertCandidate(
            c,
            _bondingFor(e3Id),
            e3Id,
            msg.sender,
            score
        );

        emit TicketSubmitted(e3Id, msg.sender, ticketNumber, score);
    }

    /// @notice Returns the request-bound sortition seed after its entropy block is sealed.
    /// @param e3Id ID of the E3 computation.
    /// @return ready Whether the seed is available.
    /// @return seed Seed used to score committee tickets.
    function sortitionSeed(
        uint256 e3Id
    ) public view returns (bool ready, uint256 seed) {
        Committee storage c = committees[e3Id];
        if (sortitionSeedResolved[e3Id]) return (true, c.seed);

        uint256 entropyBlock = sortitionEntropyBlocks[e3Id];
        bytes32 entropy;
        (ready, entropy) = RegistrySortitionLib.entropyBlockHash(
            block.chainid,
            entropyBlock
        );
        if (!ready) return (false, 0);

        seed = uint256(keccak256(abi.encode(entropy, e3Id)));
    }

    function _resolveSortitionSeed(
        uint256 e3Id,
        Committee storage c
    ) internal returns (uint256 seed) {
        (bool ready, uint256 resolvedSeed) = sortitionSeed(e3Id);
        require(
            ready,
            SortitionSeedUnavailable(e3Id, sortitionEntropyBlocks[e3Id])
        );
        if (!sortitionSeedResolved[e3Id]) {
            c.seed = resolvedSeed;
            sortitionSeedResolved[e3Id] = true;
        }
        return resolvedSeed;
    }

    /// @notice Finalize the committee after submission window closes
    /// @dev Can be called by anyone after the deadline. If threshold not met, marks E3 as failed.
    /// @param e3Id ID of the E3 computation
    /// @return success True if committee formed successfully, false if threshold not met
    function finalizeCommittee(uint256 e3Id) external returns (bool success) {
        Committee storage c = committees[e3Id];
        require(
            c.stage != ICiphernodeRegistry.CommitteeStage.None,
            CommitteeNotRequested()
        );
        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.Requested,
            CommitteeAlreadyFinalized()
        );
        require(
            block.timestamp > c.committeeDeadline,
            SubmissionWindowNotClosed()
        );
        bool thresholdMet = c.topNodes.length >= c.threshold[1];

        if (!thresholdMet) {
            c.stage = ICiphernodeRegistry.CommitteeStage.Failed;
            emit CommitteeFormationFailed(
                e3Id,
                c.topNodes.length,
                c.threshold[1]
            );
            _interfoldFor(e3Id).onE3Failed(
                e3Id,
                uint8(IInterfold.FailureReason.InsufficientCommitteeMembers)
            );
            c.obligationsReleased = true;
            _releaseCommitteeObligations(e3Id, c);
            unreleasedCommitteeCount--;
            return false;
        }

        _sortTopNodesByAscendingAddress(c);

        c.stage = ICiphernodeRegistry.CommitteeStage.Finalized;
        c.activeCount = c.topNodes.length;

        uint256 len = c.topNodes.length;
        uint256[] memory scores = new uint256[](len);
        for (uint256 i = 0; i < len; ++i) {
            address node = c.topNodes[i];
            c.memberStatus[node] = ICiphernodeRegistry.MemberStatus.Active;
            scores[i] = c.scoreOf[node];
        }

        _interfoldFor(e3Id).onCommitteeFinalized(e3Id);
        emit SortitionCommitteeFinalized(e3Id, c.topNodes, scores);
        emit CommitteeActivationChanged(e3Id, true);
        return true;
    }

    /// @inheritdoc ICiphernodeRegistry
    function releaseCommittee(uint256 e3Id) external {
        Committee storage c = committees[e3Id];
        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.Requested ||
                c.stage == ICiphernodeRegistry.CommitteeStage.Finalized,
            CommitteeNotFinalized()
        );
        if (c.obligationsReleased) {
            revert CommitteeObligationsAlreadyReleased(e3Id);
        }

        IInterfold.E3Stage stage = _interfoldFor(e3Id).getE3Stage(e3Id);
        if (
            stage != IInterfold.E3Stage.Complete &&
            stage != IInterfold.E3Stage.Failed
        ) revert E3NotTerminal(e3Id);

        c.obligationsReleased = true;
        if (c.stage == ICiphernodeRegistry.CommitteeStage.Requested) {
            c.stage = ICiphernodeRegistry.CommitteeStage.Failed;
        }
        _releaseCommitteeObligations(e3Id, c);
        unreleasedCommitteeCount--;
        emit CommitteeActivationChanged(e3Id, false);
    }

    function _releaseCommitteeObligations(
        uint256 e3Id,
        Committee storage c
    ) internal {
        IBondingRegistry e3Bonding = _bondingFor(e3Id);
        uint256 length = c.topNodes.length;
        for (uint256 i = 0; i < length; ++i) {
            e3Bonding.setCommitteeObligation(e3Id, c.topNodes[i], false);
        }
        e3Bonding.setCommitteeObligation(e3Id, address(0), false);
    }

    /// @inheritdoc ICiphernodeRegistry
    function committeeThresholdMet(uint256 e3Id) external view returns (bool) {
        Committee storage c = committees[e3Id];
        return
            c.stage == ICiphernodeRegistry.CommitteeStage.Requested &&
            c.topNodes.length >= c.threshold[1];
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Set Functions                        //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Sets the Interfold contract address
    /// @dev Only callable by owner
    /// @param _interfold Address of the Interfold contract
    function setInterfold(IInterfold _interfold) public onlyOwner {
        require(address(_interfold) != address(0), ZeroAddress());
        _requireGenerationDrained(address(interfold));
        interfold = _interfold;
        emit InterfoldSet(address(_interfold));
    }

    /// @notice Sets the bonding registry contract address
    /// @dev Only callable by owner
    /// @param _bondingRegistry Address of the bonding registry contract
    function setBondingRegistry(
        IBondingRegistry _bondingRegistry
    ) public onlyOwner {
        require(address(_bondingRegistry) != address(0), ZeroAddress());
        _requireGenerationDrained(address(bondingRegistry));
        _validateExitTiming(_bondingRegistry, exitDelayFloor());
        bondingRegistry = _bondingRegistry;
        emit BondingRegistrySet(address(_bondingRegistry));
    }

    /// @notice Sets the slashing manager contract address
    /// @dev Only callable by owner
    /// @param _slashingManager Address of the slashing manager contract
    function setSlashingManager(
        ISlashingManager _slashingManager
    ) public onlyOwner {
        require(address(_slashingManager) != address(0), ZeroAddress());
        _requireGenerationDrained(address(slashingManager));
        slashingManager = _slashingManager;
        emit RegistrySlashingManagerSet(address(_slashingManager));
    }

    /// @notice Disabled. Reverts unconditionally.
    function renounceOwnership() public view override onlyOwner {
        revert RenounceOwnershipDisabled();
    }

    /// @inheritdoc ICiphernodeRegistry
    function setSortitionSubmissionWindow(
        uint256 _sortitionSubmissionWindow
    ) public onlyOwner {
        require(
            _sortitionSubmissionWindow >= MIN_SORTITION_SUBMISSION_WINDOW &&
                _sortitionSubmissionWindow <= MAX_SORTITION_SUBMISSION_WINDOW,
            SortitionSubmissionWindowOutOfBounds(_sortitionSubmissionWindow)
        );
        uint256 requiredDelay = exitDelayFloor();
        if (_sortitionSubmissionWindow > requiredDelay) {
            requiredDelay = _sortitionSubmissionWindow;
        }
        _validateExitTiming(bondingRegistry, requiredDelay);
        sortitionSubmissionWindow = _sortitionSubmissionWindow;
        emit SortitionSubmissionWindowSet(_sortitionSubmissionWindow);
    }

    /// @inheritdoc ICiphernodeRegistry
    function exitDelayFloor() public view returns (uint256 floor) {
        floor = sortitionSubmissionWindow;
        uint256 deadline = _latestCommitteeDeadline;
        if (deadline > block.timestamp) {
            uint256 remaining = deadline - block.timestamp;
            if (remaining > floor) floor = remaining;
        }
    }

    function _validateExitTiming(
        IBondingRegistry configuredBondingRegistry,
        uint256 requiredDelay
    ) private view {
        if (address(configuredBondingRegistry) == address(0)) return;
        uint256 configuredExitDelay = configuredBondingRegistry.exitDelay();
        if (configuredExitDelay <= requiredDelay) {
            revert ExitDelayMustExceedSortitionWindow(
                configuredExitDelay,
                requiredDelay
            );
        }
    }

    function _requireGenerationDrained(address current) private view {
        if (
            current != address(0) &&
            (numCiphernodes != 0 || unreleasedCommitteeCount != 0)
        ) revert RegistryGenerationNotDrained();
    }

    /// @notice Update the registry-wide vote validity window used by accusers
    ///         when stamping `AccusationVote.deadline`.
    /// @dev Ciphernodes fetch this value at startup. Operators must restart nodes
    ///      after a reduction so peers use the same local deadline limit.
    /// @param _accusationVoteValidity New validity window in seconds.
    ///        Use the proposal and commit functions to reduce the current value.
    function setAccusationVoteValidity(
        uint256 _accusationVoteValidity
    ) external onlyOwner {
        require(
            _accusationVoteValidity >= accusationVoteValidity,
            AccusationVoteValidityDecreaseRequiresTimelock()
        );
        if (pendingAccusationVoteValidityAt != 0) {
            uint256 pending = pendingAccusationVoteValidity;
            pendingAccusationVoteValidity = 0;
            pendingAccusationVoteValidityAt = 0;
            emit AccusationVoteValidityProposalCancelled(pending);
        }
        accusationVoteValidity = _accusationVoteValidity;
        emit AccusationVoteValiditySet(_accusationVoteValidity);
    }

    /// @notice Propose a new accusation vote validity window.
    /// @dev This path permits reductions, including a zero-second window.
    function proposeAccusationVoteValidity(
        uint256 _accusationVoteValidity
    ) external onlyOwner {
        pendingAccusationVoteValidity = _accusationVoteValidity;
        pendingAccusationVoteValidityAt = block.timestamp;
        emit AccusationVoteValidityProposed(
            _accusationVoteValidity,
            block.timestamp + ACCUSATION_VOTE_VALIDITY_TIMELOCK
        );
    }

    /// @notice Commit a previously proposed accusation vote validity update.
    /// @dev The commit window lasts for one timelock period after the proposal is ready.
    /// @param _accusationVoteValidity Must match the pending proposal.
    function commitAccusationVoteValidity(
        uint256 _accusationVoteValidity
    ) external onlyOwner {
        uint256 pendingAt = pendingAccusationVoteValidityAt;
        require(pendingAt != 0, NoPendingAccusationVoteValidityUpdate());
        uint256 pending = pendingAccusationVoteValidity;
        require(
            pending == _accusationVoteValidity,
            AccusationVoteValidityMismatch(pending, _accusationVoteValidity)
        );
        uint256 readyAt = pendingAt + ACCUSATION_VOTE_VALIDITY_TIMELOCK;
        require(
            block.timestamp >= readyAt,
            AccusationVoteValidityTimelockActive(readyAt, block.timestamp)
        );
        uint256 expiredAt = readyAt + ACCUSATION_VOTE_VALIDITY_TIMELOCK;
        require(
            block.timestamp <= expiredAt,
            AccusationVoteValidityProposalExpired(expiredAt, block.timestamp)
        );
        accusationVoteValidity = _accusationVoteValidity;
        pendingAccusationVoteValidity = 0;
        pendingAccusationVoteValidityAt = 0;
        emit AccusationVoteValiditySet(_accusationVoteValidity);
    }

    /// @notice Cancel a pending accusation vote validity proposal.
    function cancelAccusationVoteValidityProposal() external onlyOwner {
        uint256 pendingAt = pendingAccusationVoteValidityAt;
        require(pendingAt != 0, NoPendingAccusationVoteValidityUpdate());
        uint256 pending = pendingAccusationVoteValidity;
        pendingAccusationVoteValidity = 0;
        pendingAccusationVoteValidityAt = 0;
        emit AccusationVoteValidityProposalCancelled(pending);
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Get Functions                        //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Check if submission window is still open for an E3
    /// @param e3Id ID of the E3 computation
    /// @return Whether the submission window is open
    function isOpen(uint256 e3Id) public view returns (bool) {
        Committee storage c = committees[e3Id];
        if (c.stage != ICiphernodeRegistry.CommitteeStage.Requested)
            return false;
        return block.timestamp <= c.committeeDeadline;
    }

    /// @inheritdoc ICiphernodeRegistry
    function committeePublicKey(
        uint256 e3Id
    ) external view returns (bytes32 publicKeyHash) {
        publicKeyHash = publicKeyHashes[e3Id];
        require(publicKeyHash != bytes32(0), CommitteeNotPublished());
    }

    /// @inheritdoc ICiphernodeRegistry
    /// @dev This global view does not predict ticket acceptance for an existing E3.
    function isCiphernodeEligible(address node) public view returns (bool) {
        if (!isEnabled(node)) return false;

        require(
            address(bondingRegistry) != address(0),
            BondingRegistryNotSet()
        );
        return bondingRegistry.isActive(node);
    }

    /// @inheritdoc ICiphernodeRegistry
    function isEnabled(address node) public view returns (bool) {
        return ciphernodeEnabled[node];
    }

    /// @notice Returns the current root of the ciphernode IMT
    /// @return Current IMT root
    function root() public view returns (uint256) {
        return ciphernodes._root(TREE_DEPTH);
    }

    /// @notice Returns the IMT root at the time a committee was requested
    /// @param e3Id ID of the E3
    /// @return IMT root at time of committee request
    function rootAt(uint256 e3Id) public view returns (uint256) {
        return roots[e3Id];
    }

    /// @inheritdoc ICiphernodeRegistry
    function getCommitteeNodes(
        uint256 e3Id
    ) public view returns (address[] memory nodes) {
        Committee storage c = committees[e3Id];
        require(c.publicKey != bytes32(0), CommitteeNotPublished());
        nodes = c.topNodes;
    }

    /// @inheritdoc ICiphernodeRegistry
    function getCommitteeHash(
        uint256 e3Id
    ) public view returns (bytes32 committeeHash) {
        Committee storage c = committees[e3Id];
        require(c.publicKey != bytes32(0), CommitteeNotPublished());
        committeeHash = c.committeeHash;
    }

    /// @inheritdoc ICiphernodeRegistry
    function getDkgAnchors(
        uint256 e3Id
    )
        external
        view
        returns (
            uint256[] memory partyIds,
            bytes32[] memory skAggCommits,
            bytes32[] memory esmAggCommits
        )
    {
        require(publicKeyHashes[e3Id] != bytes32(0), CommitteeNotPublished());
        return (
            dkgPartyIds[e3Id],
            dkgSkAggCommits[e3Id],
            dkgEsmAggCommits[e3Id]
        );
    }

    /// @notice Returns the current size of the ciphernode IMT
    /// @return Size of the IMT
    function treeSize() public view returns (uint256) {
        return ciphernodes.numberOfLeaves;
    }

    /// @notice Returns the address of the bonding registry
    /// @return Address of the bonding registry contract
    function getBondingRegistry() external view returns (address) {
        return address(bondingRegistry);
    }

    /// @inheritdoc ICiphernodeRegistry
    function getCommitteeDeadline(
        uint256 e3Id
    ) external view returns (uint256) {
        Committee storage c = committees[e3Id];
        require(
            c.stage != ICiphernodeRegistry.CommitteeStage.None,
            CommitteeNotRequested()
        );
        return c.committeeDeadline;
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //              Committee Expulsion Functions             //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @inheritdoc ICiphernodeRegistry
    function expelCommitteeMember(
        uint256 e3Id,
        address node,
        bytes32 reason
    ) external returns (uint256 activeCount, uint32 thresholdM) {
        require(
            msg.sender == address(_slashingManagerFor(e3Id)),
            NotSlashingManager()
        );
        Committee storage c = committees[e3Id];
        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.Finalized,
            CommitteeNotFinalized()
        );
        thresholdM = c.threshold[0];

        // Idempotent: if already expelled (or never a member), return current state
        if (c.memberStatus[node] != ICiphernodeRegistry.MemberStatus.Active) {
            activeCount = c.activeCount;
            return (activeCount, thresholdM);
        }

        c.memberStatus[node] = ICiphernodeRegistry.MemberStatus.Expelled;
        c.activeCount--;

        activeCount = c.activeCount;
        emit CommitteeMemberExpelled(e3Id, node, reason, activeCount);

        // Emit viability update
        bool viable = activeCount >= thresholdM;
        emit CommitteeViabilityUpdated(e3Id, activeCount, thresholdM, viable);
    }

    /// @inheritdoc ICiphernodeRegistry
    function isCommitteeMemberActive(
        uint256 e3Id,
        address node
    ) external view returns (bool) {
        Committee storage c = committees[e3Id];
        return
            c.stage == ICiphernodeRegistry.CommitteeStage.Finalized &&
            c.memberStatus[node] == ICiphernodeRegistry.MemberStatus.Active;
    }

    /// @inheritdoc ICiphernodeRegistry
    function isCommitteeMember(
        uint256 e3Id,
        address node
    ) external view returns (bool) {
        Committee storage c = committees[e3Id];
        return
            c.stage == ICiphernodeRegistry.CommitteeStage.Finalized &&
            c.memberStatus[node] != ICiphernodeRegistry.MemberStatus.None;
    }

    /// @inheritdoc ICiphernodeRegistry
    function canonicalCommitteeNodeAt(
        uint256 e3Id,
        uint256 partyId
    ) external view returns (address) {
        Committee storage c = committees[e3Id];
        // Only expose `partyId -> node` for canonical (finalized) committees.
        // Pre-finalization, `topNodes` is still being populated by sortition
        // and is not the canonical mapping.
        require(
            c.stage == ICiphernodeRegistry.CommitteeStage.Finalized,
            CommitteeNotFinalized()
        );
        require(
            partyId < c.topNodes.length,
            PartyIdOutOfBounds(partyId, c.topNodes.length)
        );
        return c.topNodes[partyId];
    }

    /// @inheritdoc ICiphernodeRegistry
    function getActiveCommitteeNodes(
        uint256 e3Id
    ) external view returns (address[] memory nodes, uint256[] memory scores) {
        Committee storage c = committees[e3Id];
        if (c.stage != ICiphernodeRegistry.CommitteeStage.Finalized) {
            return (new address[](0), new uint256[](0));
        }

        uint256 total = c.topNodes.length;
        uint256 actCount = 0;
        for (uint256 i = 0; i < total; ++i) {
            if (
                c.memberStatus[c.topNodes[i]] ==
                ICiphernodeRegistry.MemberStatus.Active
            ) {
                actCount++;
            }
        }

        nodes = new address[](actCount);
        scores = new uint256[](actCount);
        uint256 idx = 0;
        for (uint256 i = 0; i < total; ++i) {
            address node = c.topNodes[i];
            if (
                c.memberStatus[node] == ICiphernodeRegistry.MemberStatus.Active
            ) {
                nodes[idx] = node;
                scores[idx] = c.scoreOf[node];
                idx++;
            }
        }
    }

    /// @inheritdoc ICiphernodeRegistry
    function getCommitteeViability(
        uint256 e3Id
    )
        external
        view
        returns (
            uint256 activeCount,
            uint32 thresholdM,
            uint32 thresholdN,
            bool viable
        )
    {
        Committee storage c = committees[e3Id];
        activeCount = c.activeCount;
        thresholdM = c.threshold[0];
        thresholdN = c.threshold[1];
        viable = activeCount >= thresholdM;
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //                   Internal Functions                   //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice Computes ticket score as keccak256(node || ticketNumber || e3Id || seed)
    /// @param node Address of the ciphernode
    /// @param ticketNumber The ticket number
    /// @param e3Id ID of the E3 computation
    /// @param seed Random seed for the E3
    /// @return score The computed score
    function _computeTicketScore(
        address node,
        uint256 ticketNumber,
        uint256 e3Id,
        uint256 seed
    ) internal pure returns (uint256) {
        bytes32 hash = keccak256(
            abi.encodePacked(node, ticketNumber, e3Id, seed)
        );
        return uint256(hash);
    }

    /// @notice Validates that a node is eligible to submit a ticket
    /// @dev Uses ticket voting power at `requestBlock - 1` for deterministic validation.
    ///      The ticket token uses an EIP-6372 timestamp clock, so changes at the request timestamp
    ///      are excluded even when their logs precede the request event.
    /// @param node Address of the ciphernode
    /// @param ticketNumber The ticket number being submitted
    /// @param e3Id ID of the E3 computation
    function _validateNodeEligibility(
        address node,
        uint256 ticketNumber,
        uint256 e3Id
    ) internal view {
        require(ticketNumber > 0, InvalidTicketNumber());
        IBondingRegistry e3Bonding = _bondingFor(e3Id);
        require(address(e3Bonding) != address(0), BondingRegistryNotSet());

        Committee storage c = committees[e3Id];

        // Bind ticket weight to the request-time snapshot through the ticket
        // token's EIP-6372 ERC20Votes checkpoints. {submitTicket} uses this
        // E3's saved bonding registry for the current `isActive` check. The
        // score and selection weight below use only the historical ticket
        // balance at `c.requestBlock - 1`. Later balance changes cannot
        // increase the saved weight.
        uint256 ticketBalance = e3Bonding.ticketToken().getPastVotes(
            node,
            c.requestBlock - 1
        );
        uint256 ticketPrice = sortitionTicketPrices[e3Id];

        require(ticketPrice > 0, InvalidTicketNumber());
        uint256 availableTickets = ticketBalance / ticketPrice;

        require(availableTickets > 0, NodeNotEligible());
        require(ticketNumber <= availableTickets, InvalidTicketNumber());
    }

    function _bondingFor(
        uint256 e3Id
    ) internal view returns (IBondingRegistry e3Bonding) {
        return _committeeDependencies[e3Id].bonding;
    }

    function _interfoldFor(
        uint256 e3Id
    ) internal view returns (IInterfold e3Interfold) {
        return _committeeDependencies[e3Id].interfoldContract;
    }

    function _slashingManagerFor(
        uint256 e3Id
    ) internal view returns (ISlashingManager e3SlashingManager) {
        return _committeeDependencies[e3Id].slashManager;
    }

    function _dkgFoldAttestationVerifierFor(
        uint256 e3Id
    )
        internal
        view
        returns (IDkgFoldAttestationVerifier e3DkgFoldAttestationVerifier)
    {
        return _committeeDependencies[e3Id].dkgFoldAttestationVerifier;
    }

    /// @inheritdoc ICiphernodeRegistry
    function dkgFoldAttestationVerifierFor(
        uint256 e3Id
    ) external view returns (IDkgFoldAttestationVerifier) {
        return _dkgFoldAttestationVerifierFor(e3Id);
    }

    /// @notice Sort `topNodes` by ascending address before committee finalization.
    /// @dev Canonical address-ascending order so `CommitteeHashLib.hash(topNodes)`
    ///      matches what off-chain aggregators independently compute over the same
    ///      address set (Rust uses `BTreeSet<String>` which iterates lexicographically,
    ///      equivalent to numeric address-ascending for hex-encoded addresses).
    ///      This also defines `party_id` = position in the address-sorted committee.
    /// @param c Committee storage reference
    function _sortTopNodesByAscendingAddress(Committee storage c) internal {
        uint256 len = c.topNodes.length;
        for (uint256 i = 0; i < len; ++i) {
            for (uint256 j = i + 1; j < len; ++j) {
                address left = c.topNodes[i];
                address right = c.topNodes[j];
                if (right < left) {
                    c.topNodes[i] = right;
                    c.topNodes[j] = left;
                }
            }
        }
    }

    ////////////////////////////////////////////////////////////
    //                                                        //
    //              ERC-165 Interface Detection               //
    //                                                        //
    ////////////////////////////////////////////////////////////

    /// @notice ERC-165 interface detection. Advertises
    ///         {ICiphernodeRegistry} and {IERC165}.
    function supportsInterface(
        bytes4 interfaceId
    ) external pure virtual returns (bool) {
        return
            interfaceId == type(ICiphernodeRegistry).interfaceId ||
            interfaceId == type(IERC165).interfaceId;
    }

    /// @dev Reserved storage slots for future upgrades.
    // solhint-disable-next-line var-name-mixedcase
    uint256[44] private __gap;
}
