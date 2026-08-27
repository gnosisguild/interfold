// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { Ownable2Step } from "@openzeppelin/contracts/access/Ownable2Step.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { ICiphernodeRegistry } from "../interfaces/ICiphernodeRegistry.sol";
import { INodeReleaseManager } from "../interfaces/INodeReleaseManager.sol";
import { INodeReleaseRegistry } from "../interfaces/INodeReleaseRegistry.sol";

/// @notice Governance policy for ciphernode software releases.
/// @dev Release acknowledgement prevents accidental stale participation. It does not prove which
///      binary a malicious operator runs.
contract NodeReleaseRegistry is INodeReleaseRegistry, Ownable2Step {
    // solhint-disable-next-line immutable-vars-naming
    IBondingRegistry public immutable bondingRegistry;
    // solhint-disable-next-line immutable-vars-naming
    ICiphernodeRegistry public immutable ciphernodeRegistry;

    uint32 public requiredProtocolVersion;
    uint32 public requiredNodeGeneration;
    bytes32 public recommendedNodeReleaseId;

    mapping(bytes32 releaseId => NodeRelease release) private _releases;
    mapping(address operator => bytes32 releaseId) public operatorNodeReleaseId;

    constructor(
        address owner,
        IBondingRegistry bonding,
        ICiphernodeRegistry ciphernodeRegistry_
    ) Ownable(owner) {
        if (
            owner == address(0) ||
            address(bonding).code.length == 0 ||
            address(ciphernodeRegistry_).code.length == 0
        ) revert InvalidNodeRelease();
        bondingRegistry = bonding;
        ciphernodeRegistry = ciphernodeRegistry_;
    }

    /// @notice Disabled so release administration always has a recoverable owner.
    function renounceOwnership() public pure override {
        revert RenounceOwnershipDisabled();
    }

    function approveNodeRelease(
        bytes32 releaseId,
        uint32 protocolVersion,
        uint32 nodeGeneration
    ) external onlyOwner {
        if (
            releaseId == bytes32(0) ||
            protocolVersion == 0 ||
            nodeGeneration == 0
        ) revert InvalidNodeRelease();
        NodeRelease storage release = _releases[releaseId];
        if (
            release.protocolVersion != 0 &&
            (release.protocolVersion != protocolVersion ||
                release.nodeGeneration != nodeGeneration)
        ) revert NodeReleaseMetadataMismatch(releaseId);
        if (release.protocolVersion != 0 && !release.approved) {
            assertUpgradeWindow();
        }
        release.protocolVersion = protocolVersion;
        release.nodeGeneration = nodeGeneration;
        release.approved = true;
        emit NodeReleaseApprovalUpdated(
            releaseId,
            protocolVersion,
            nodeGeneration,
            true
        );
    }

    function revokeNodeRelease(bytes32 releaseId) external onlyOwner {
        assertUpgradeWindow();
        NodeRelease storage release = _releases[releaseId];
        if (!release.approved) revert NodeReleaseNotApproved(releaseId);
        release.approved = false;
        if (recommendedNodeReleaseId == releaseId) {
            recommendedNodeReleaseId = bytes32(0);
            emit RecommendedNodeReleaseUpdated(bytes32(0));
        }
        bondingRegistry.refreshOperatorStatus(address(0));
        emit NodeReleaseApprovalUpdated(
            releaseId,
            release.protocolVersion,
            release.nodeGeneration,
            false
        );
    }

    function setRequiredNodeRelease(bytes32 releaseId) external onlyOwner {
        assertUpgradeWindow();
        NodeRelease storage release = _releases[releaseId];
        if (!release.approved) revert NodeReleaseNotApproved(releaseId);
        uint32 previousProtocolVersion = requiredProtocolVersion;
        uint32 previousNodeGeneration = requiredNodeGeneration;
        if (
            release.protocolVersion < previousProtocolVersion ||
            release.nodeGeneration < previousNodeGeneration ||
            (release.protocolVersion == previousProtocolVersion &&
                release.nodeGeneration == previousNodeGeneration)
        ) revert NodeReleasePolicyRegression();
        requiredProtocolVersion = release.protocolVersion;
        requiredNodeGeneration = release.nodeGeneration;
        recommendedNodeReleaseId = releaseId;
        bondingRegistry.refreshOperatorStatus(address(0));
        emit RequiredNodeReleaseUpdated(
            previousProtocolVersion,
            previousNodeGeneration,
            release.protocolVersion,
            release.nodeGeneration,
            releaseId
        );
        emit RecommendedNodeReleaseUpdated(releaseId);
    }

    function setRecommendedNodeRelease(bytes32 releaseId) external onlyOwner {
        if (releaseId != bytes32(0)) {
            NodeRelease storage release = _releases[releaseId];
            if (!release.approved) revert NodeReleaseNotApproved(releaseId);
            if (
                release.protocolVersion != requiredProtocolVersion ||
                release.nodeGeneration < requiredNodeGeneration
            ) revert NodeReleaseNotCompatible(releaseId);
        }
        recommendedNodeReleaseId = releaseId;
        emit RecommendedNodeReleaseUpdated(releaseId);
    }

    function acknowledgeNodeRelease(bytes32 releaseId) external {
        if (!_releases[releaseId].approved) {
            revert NodeReleaseNotApproved(releaseId);
        }
        operatorNodeReleaseId[msg.sender] = releaseId;
        emit OperatorNodeReleaseAcknowledged(msg.sender, releaseId);
        if (bondingRegistry.isRegistered(msg.sender)) {
            bondingRegistry.refreshOperatorStatus(msg.sender);
        }
    }

    function getNodeRelease(
        bytes32 releaseId
    ) external view returns (NodeRelease memory) {
        return _releases[releaseId];
    }

    function isNodeReleaseReady(address operator) public view returns (bool) {
        NodeRelease storage release = _releases[
            operatorNodeReleaseId[operator]
        ];
        return
            requiredProtocolVersion != 0 &&
            requiredNodeGeneration != 0 &&
            release.approved &&
            release.protocolVersion == requiredProtocolVersion &&
            release.nodeGeneration >= requiredNodeGeneration;
    }

    function assertUpgradeWindow() public view {
        if (!ciphernodeRegistry.interfold().requestsPaused()) {
            revert NodeReleasePolicyRequiresPause();
        }
        uint256 activeE3s = ciphernodeRegistry.interfold().activeE3Count();
        uint256 unreleasedCommittees = ciphernodeRegistry
            .unreleasedCommitteeCount();
        if (activeE3s != 0 || unreleasedCommittees != 0) {
            revert NodeReleasePolicyInUse(activeE3s, unreleasedCommittees);
        }
    }

    function activate() external {
        assertUpgradeWindow();
        address interfold = address(ciphernodeRegistry.interfold());
        if (
            msg.sender != interfold ||
            address(INodeReleaseManager(interfold).nodeReleaseRegistry()) !=
            address(this) ||
            address(INodeReleaseManager(interfold).bondingRegistry()) !=
            address(bondingRegistry) ||
            address(INodeReleaseManager(interfold).ciphernodeRegistry()) !=
            address(ciphernodeRegistry)
        ) revert NodeReleaseBindingMismatch();
        bondingRegistry.refreshOperatorStatus(address(0));
    }
}
