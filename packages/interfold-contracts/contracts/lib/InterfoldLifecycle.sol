// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

import { IInterfold } from "../interfaces/IInterfold.sol";
import { ICiphernodeRegistry } from "../interfaces/ICiphernodeRegistry.sol";
import { IDecryptionVerifier } from "../interfaces/IDecryptionVerifier.sol";

/**
 * @title InterfoldLifecycle
 * @notice Contains stateless E3 lifecycle validation and proof helpers.
 * @dev External calls use DELEGATECALL. This keeps the Interfold proxy as the
 *      execution context and keeps lifecycle code out of its runtime bytecode.
 */
library InterfoldLifecycle {
    function validateRegistryCaller(
        address caller,
        address registry
    ) external pure {
        require(caller == registry, IInterfold.OnlyCiphernodeRegistry());
    }

    function validateSlashCaller(
        address caller,
        address slashManager
    ) external pure {
        require(caller == slashManager, IInterfold.OnlySlashingManager());
    }

    function validateRegistryOrSlashCaller(
        address caller,
        address registry,
        address slashManager
    ) external pure {
        require(
            caller == registry || caller == slashManager,
            IInterfold.OnlyCiphernodeRegistryOrSlashingManager()
        );
    }

    function verifyPlaintext(
        address verifierAddress,
        address registryAddress,
        uint256 e3Id,
        bytes32 ciphertextHash,
        bytes32 committeePublicKey,
        bytes32 plaintextHash,
        bytes32 ciphertextCommitment,
        bytes calldata proof
    ) external view {
        if (proof.length == 0) revert IInterfold.ProofRequired();
        bytes32 committeeHash = ICiphernodeRegistry(registryAddress)
            .getCommitteeHash(e3Id);
        bytes32 decryptionDomain = keccak256(
            abi.encode(
                block.chainid,
                address(this),
                e3Id,
                committeeHash,
                ciphertextHash,
                committeePublicKey
            )
        );
        if (
            !IDecryptionVerifier(verifierAddress).verify(
                e3Id,
                decryptionDomain,
                plaintextHash,
                committeeHash,
                ciphertextCommitment,
                proof
            )
        ) revert IDecryptionVerifier.InvalidProof();
    }

    // prettier-ignore
    function validateCommitteePublication(
        address caller, address registry, uint256 e3Id, uint8 current, uint256 dkgDeadline
    ) external view {
        if (caller != registry) revert IInterfold.OnlyCiphernodeRegistry();
        IInterfold.E3Stage stage = IInterfold.E3Stage(current);
        if (stage != IInterfold.E3Stage.CommitteeFinalized)
            revert IInterfold.InvalidStage(e3Id, IInterfold.E3Stage.CommitteeFinalized, stage);
        if (block.timestamp > dkgDeadline)
            revert IInterfold.DKGDeadlinePassed(e3Id, dkgDeadline);
    }

    function honestNodes(
        address registryAddress,
        uint256 e3Id
    ) external view returns (address[] memory) {
        (address[] memory nodes, ) = ICiphernodeRegistry(registryAddress)
            .getActiveCommitteeNodes(e3Id);
        return nodes;
    }

    /// @notice Checks the publication gates for a ciphertext output.
    /// @param current The current E3 stage, encoded as `uint8`.
    function validatePublishCiphertext(
        uint256 e3Id,
        uint8 current,
        uint256 computeDeadline,
        uint256 inputWindowEnd,
        bytes32 ciphertextOutput,
        uint256 nowTs
    ) external pure {
        IInterfold.E3Stage stage = IInterfold.E3Stage(current);
        if (stage != IInterfold.E3Stage.KeyPublished)
            revert IInterfold.InvalidStage(
                e3Id,
                IInterfold.E3Stage.KeyPublished,
                stage
            );
        if (computeDeadline < nowTs)
            revert IInterfold.CommitteeDutiesCompleted(e3Id, computeDeadline);
        if (nowTs < inputWindowEnd)
            revert IInterfold.InputDeadlineNotReached(e3Id, inputWindowEnd);
        if (ciphertextOutput != bytes32(0))
            revert IInterfold.CiphertextOutputAlreadyPublished(e3Id);
    }

    /// @notice Checks whether an E3 stage can enter the failure path.
    /// @param current The current E3 stage, encoded as `uint8`.
    function validateMarkFailedStage(
        uint256 e3Id,
        uint8 current
    ) external pure {
        IInterfold.E3Stage stage = IInterfold.E3Stage(current);
        if (stage == IInterfold.E3Stage.None)
            revert IInterfold.InvalidStage(
                e3Id,
                IInterfold.E3Stage.Requested,
                stage
            );
        if (stage == IInterfold.E3Stage.Complete)
            revert IInterfold.E3AlreadyComplete(e3Id);
        if (stage == IInterfold.E3Stage.Failed)
            revert IInterfold.E3AlreadyFailed(e3Id);
    }

    // prettier-ignore
    function validateReportedFailure(
        address caller, address registry, address slashManager, uint256 e3Id, uint8 current, uint8 reason
    ) external pure {
        if (caller != registry && caller != slashManager)
            revert IInterfold.OnlyCiphernodeRegistryOrSlashingManager();
        IInterfold.E3Stage stage = IInterfold.E3Stage(current);
        if (stage == IInterfold.E3Stage.None)
            revert IInterfold.InvalidStage(e3Id, IInterfold.E3Stage.Requested, stage);
        if (stage == IInterfold.E3Stage.Complete)
            revert IInterfold.E3AlreadyComplete(e3Id);
        if (stage == IInterfold.E3Stage.Failed)
            revert IInterfold.E3AlreadyFailed(e3Id);
        if (
            reason == uint8(IInterfold.FailureReason.None) ||
            reason >= uint8(IInterfold.FailureReason._MAX_FAILURE_REASON)
        ) revert IInterfold.InvalidFailureReason(reason);
    }

    function validateMarkFailedCaller(
        uint256 e3Id,
        uint256 deadline,
        uint256 grace,
        address caller,
        address requester,
        address contractOwner,
        address registry
    ) external view {
        if (grace == 0) return;
        uint256 graceEnds = deadline + grace;
        if (
            block.timestamp < graceEnds &&
            caller != requester &&
            caller != contractOwner &&
            !ICiphernodeRegistry(registry).isCommitteeMemberActive(e3Id, caller)
        ) revert IInterfold.MarkE3FailedInGracePeriod(e3Id, graceEnds);
    }

    // prettier-ignore
    function stageDeadlineAndReason(
        address registryAddress, uint256 e3Id, uint8 current, IInterfold.E3Deadlines calldata deadlines
    ) external view returns (uint256 deadline, uint8 reason) {
        IInterfold.E3Stage stage = IInterfold.E3Stage(current);
        if (stage == IInterfold.E3Stage.Requested)
            return (
                ICiphernodeRegistry(registryAddress).getCommitteeDeadline(e3Id),
                uint8(IInterfold.FailureReason.CommitteeFormationTimeout)
            );
        if (stage == IInterfold.E3Stage.CommitteeFinalized)
            return (
                deadlines.dkgDeadline,
                uint8(IInterfold.FailureReason.DKGTimeout)
            );
        if (stage == IInterfold.E3Stage.KeyPublished)
            return (
                deadlines.computeDeadline,
                uint8(IInterfold.FailureReason.ComputeTimeout)
            );
        if (stage == IInterfold.E3Stage.CiphertextReady)
            return (
                deadlines.decryptionDeadline,
                uint8(IInterfold.FailureReason.DecryptionTimeout)
            );
    }

    /// @notice Checks the timeout configuration.
    function validateTimeoutConfig(
        IInterfold.E3TimeoutConfig calldata config,
        uint256 maxTimeoutWindow
    ) external pure {
        if (config.dkgWindow == 0 || config.dkgWindow > maxTimeoutWindow)
            revert IInterfold.InvalidTimeoutWindow();
        if (
            config.computeWindow == 0 || config.computeWindow > maxTimeoutWindow
        ) revert IInterfold.InvalidTimeoutWindow();
        if (
            config.decryptionWindow == 0 ||
            config.decryptionWindow > maxTimeoutWindow
        ) revert IInterfold.InvalidTimeoutWindow();
    }

    /// @notice Checks the committee threshold configuration.
    function validateCommitteeThresholds(
        uint32[2] calldata threshold,
        uint32 minCommitteeSize,
        uint32 minThreshold,
        uint32 maxCommitteeSize
    ) external pure {
        if (threshold[0] == 0 || threshold[1] < threshold[0])
            revert IInterfold.InvalidThresholdValues();
        if (threshold[1] > maxCommitteeSize)
            revert IInterfold.InvalidThresholdValues();
        if (minCommitteeSize > 0 && threshold[1] < minCommitteeSize)
            revert IInterfold.BelowMinCommitteeSize(
                threshold[1],
                minCommitteeSize
            );
        if (minThreshold > 0 && threshold[0] < minThreshold)
            revert IInterfold.BelowMinThreshold(threshold[0], minThreshold);
    }

    /// @notice Checks the request input window and total duration.
    function validateRequest(
        uint256[2] calldata inputWindow,
        uint256 nowTs,
        uint256 computeWindow,
        uint256 decryptionWindow,
        uint256 maxDuration
    ) external pure {
        if (inputWindow[0] < nowTs)
            revert IInterfold.InvalidInputDeadlineStart(inputWindow[0]);
        if (inputWindow[1] < inputWindow[0])
            revert IInterfold.InvalidInputDeadlineEnd(inputWindow[1]);
        uint256 totalDuration = inputWindow[1] -
            nowTs +
            computeWindow +
            decryptionWindow;
        if (totalDuration > maxDuration)
            revert IInterfold.InvalidDuration(totalDuration);
    }
}
