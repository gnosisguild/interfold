// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { IBondingRegistry } from "./IBondingRegistry.sol";
import { ICiphernodeRegistry } from "./ICiphernodeRegistry.sol";

/// @notice Controls which ciphernode releases can participate in new E3s.
interface INodeReleaseRegistry {
    /// @notice Metadata for one governance-approved ciphernode release.
    struct NodeRelease {
        uint32 protocolVersion;
        uint32 nodeGeneration;
        bool approved;
    }

    error InvalidNodeRelease();
    error NodeReleaseNotApproved(bytes32 releaseId);
    error NodeReleaseNotCompatible(bytes32 releaseId);
    error NodeReleaseMetadataMismatch(bytes32 releaseId);
    error NodeReleasePolicyRegression();
    error NodeReleasePolicyRequiresPause();
    error NodeReleasePolicyInUse(
        uint256 activeE3s,
        uint256 unreleasedCommittees
    );
    error NodeReleaseBindingMismatch();
    error OnlyNodeReleaseRegistry();
    error RenounceOwnershipDisabled();

    event NodeReleaseApprovalUpdated(
        bytes32 indexed releaseId,
        uint32 protocolVersion,
        uint32 nodeGeneration,
        bool approved
    );
    event RequiredNodeReleaseUpdated(
        uint32 previousProtocolVersion,
        uint32 previousNodeGeneration,
        uint32 requiredProtocolVersion,
        uint32 requiredNodeGeneration,
        bytes32 indexed releaseId
    );
    event RecommendedNodeReleaseUpdated(bytes32 indexed releaseId);
    event OperatorNodeReleaseAcknowledged(
        address indexed operator,
        bytes32 indexed releaseId
    );

    /// @notice Approves an immutable release identifier and its compatibility versions.
    function approveNodeRelease(
        bytes32 releaseId,
        uint32 protocolVersion,
        uint32 nodeGeneration
    ) external;

    /// @notice Revokes a release and invalidates cached operator eligibility.
    function revokeNodeRelease(bytes32 releaseId) external;

    /// @notice Makes an approved release the minimum version for new work.
    function setRequiredNodeRelease(bytes32 releaseId) external;

    /// @notice Sets the preferred compatible release without excluding older compatible releases.
    function setRecommendedNodeRelease(bytes32 releaseId) external;

    /// @notice Records the release claimed by the calling operator and refreshes its eligibility.
    function acknowledgeNodeRelease(bytes32 releaseId) external;

    function getNodeRelease(
        bytes32 releaseId
    ) external view returns (NodeRelease memory);

    function requiredProtocolVersion() external view returns (uint32);

    function requiredNodeGeneration() external view returns (uint32);

    function recommendedNodeReleaseId() external view returns (bytes32);

    function operatorNodeReleaseId(
        address operator
    ) external view returns (bytes32);

    function isNodeReleaseReady(address operator) external view returns (bool);

    function bondingRegistry() external view returns (IBondingRegistry);

    function ciphernodeRegistry() external view returns (ICiphernodeRegistry);

    /// @notice Reverts unless node eligibility can change without affecting an active E3.
    function assertUpgradeWindow() external view;

    /// @notice Verifies its protocol binding and invalidates cached node eligibility.
    function activate() external;
}
