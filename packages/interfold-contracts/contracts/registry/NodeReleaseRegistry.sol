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

/// @notice Governance policy for ciphernode compatibility versions.
/// @dev Acknowledgement prevents accidental stale participation. It does not prove which binary a
///      malicious operator runs.
contract NodeReleaseRegistry is INodeReleaseRegistry, Ownable2Step {
    // solhint-disable-next-line immutable-vars-naming
    IBondingRegistry public immutable bondingRegistry;
    // solhint-disable-next-line immutable-vars-naming
    ICiphernodeRegistry public immutable ciphernodeRegistry;

    uint32 public requiredProtocolVersion;
    uint32 public requiredNodeGeneration;

    mapping(address operator => OperatorNodeRelease release)
        private _operatorNodeReleases;

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

    function setRequiredNodeRelease(
        uint32 protocolVersion,
        uint32 nodeGeneration
    ) external onlyOwner {
        if (protocolVersion == 0 || nodeGeneration == 0) {
            revert InvalidNodeRelease();
        }
        assertUpgradeWindow();
        uint32 previousProtocolVersion = requiredProtocolVersion;
        uint32 previousNodeGeneration = requiredNodeGeneration;
        if (
            protocolVersion < previousProtocolVersion ||
            nodeGeneration < previousNodeGeneration ||
            (protocolVersion == previousProtocolVersion &&
                nodeGeneration == previousNodeGeneration)
        ) revert NodeReleasePolicyRegression();
        requiredProtocolVersion = protocolVersion;
        requiredNodeGeneration = nodeGeneration;
        if (previousProtocolVersion != 0) {
            bondingRegistry.refreshOperatorStatus(address(0));
        }
        emit RequiredNodeReleaseUpdated(
            previousProtocolVersion,
            previousNodeGeneration,
            protocolVersion,
            nodeGeneration
        );
    }

    function acknowledgeNodeRelease(
        bytes32 releaseId,
        uint32 protocolVersion,
        uint32 nodeGeneration
    ) external {
        if (
            releaseId == bytes32(0) ||
            protocolVersion == 0 ||
            nodeGeneration == 0
        ) {
            revert InvalidNodeRelease();
        }
        _operatorNodeReleases[msg.sender] = OperatorNodeRelease(
            releaseId,
            protocolVersion,
            nodeGeneration
        );
        emit OperatorNodeReleaseAcknowledged(
            msg.sender,
            releaseId,
            protocolVersion,
            nodeGeneration
        );
        if (bondingRegistry.isRegistered(msg.sender)) {
            bondingRegistry.refreshOperatorStatus(msg.sender);
        }
    }

    function isNodeReleaseReady(address operator) public view returns (bool) {
        OperatorNodeRelease storage release = _operatorNodeReleases[operator];
        return
            requiredProtocolVersion != 0 &&
            requiredNodeGeneration != 0 &&
            release.protocolVersion == requiredProtocolVersion &&
            release.nodeGeneration >= requiredNodeGeneration;
    }

    function operatorNodeRelease(
        address operator
    ) external view returns (OperatorNodeRelease memory) {
        return _operatorNodeReleases[operator];
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
