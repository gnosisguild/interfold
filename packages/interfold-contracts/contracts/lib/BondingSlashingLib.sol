// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import {
    IERC165
} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { IInterfold } from "../interfaces/IInterfold.sol";
import { ISlashingManager } from "../interfaces/ISlashingManager.sol";
import {
    BONDING_SLASHING_STORAGE_SLOT,
    BondingSlashLock,
    BondingSlashingStorage,
    SlashingManagerObligations
} from "../storage/BondingSlashingStorage.sol";

interface IRegistryMigrationView {
    function interfold() external view returns (address);

    function getBondingRegistry() external view returns (address);

    function slashingManager() external view returns (address);

    function dkgFoldAttestationVerifier() external view returns (address);

    function numCiphernodes() external view returns (uint256);

    function root() external view returns (uint256);
}

interface IInterfoldMigrationView {
    function feeToken() external view returns (address);

    function isFeeTokenAllowed(address token) external view returns (bool);
}

/// @notice Stores manager-owned slash locks and bans in BondingRegistry storage.
library BondingSlashingLib {
    uint256 private constant API_VERSION = 1;
    uint256 private constant PROBE_GAS = 100_000;

    function openSlashLockCount(
        address operator
    ) external view returns (uint256) {
        return _layout().openSlashLocks[operator];
    }

    function activeBanCount(address operator) external view returns (uint256) {
        return _layout().activeBans[operator];
    }

    function validateExitClaim(address operator) external view {
        BondingSlashingStorage.Layout storage state = _layout();
        if (state.openSlashLocks[operator] != 0) {
            revert IBondingRegistry.OperatorUnderSlash();
        }
        if (state.unresolvedCommittees[operator] != 0) {
            revert IBondingRegistry.OperatorInActiveCommittee();
        }
    }

    function setCommitteeObligation(
        address currentRegistry,
        uint256 e3Id,
        address operator,
        bool active
    ) external {
        BondingSlashingStorage.Layout storage state = _layout();
        if (active) {
            _openCommitteeObligation(state, currentRegistry, e3Id, operator);
        } else {
            _releaseCommitteeObligation(state, e3Id, operator);
        }

        emit IBondingRegistry.CommitteeObligationUpdated(
            e3Id,
            msg.sender,
            operator,
            active
        );
    }

    function _openCommitteeObligation(
        BondingSlashingStorage.Layout storage state,
        address currentRegistry,
        uint256 e3Id,
        address operator
    ) private {
        address assignedRegistry = state.committeeRegistries[e3Id];
        if (assignedRegistry == address(0)) {
            if (
                currentRegistry == address(0) || msg.sender != currentRegistry
            ) {
                revert IBondingRegistry.Unauthorized();
            }
            state.committeeRegistries[e3Id] = msg.sender;
            state.activeCommitteeAssignments++;
        } else if (msg.sender != assignedRegistry) {
            revert IBondingRegistry.Unauthorized();
        }

        if (
            operator == address(0) || state.committeeObligations[e3Id][operator]
        ) return;
        state.committeeObligations[e3Id][operator] = true;
        state.committeeMemberCounts[e3Id]++;
        state.unresolvedCommittees[operator]++;
    }

    function _releaseCommitteeObligation(
        BondingSlashingStorage.Layout storage state,
        uint256 e3Id,
        address operator
    ) private {
        address assignedRegistry = state.committeeRegistries[e3Id];
        if (assignedRegistry == address(0) || msg.sender != assignedRegistry) {
            revert IBondingRegistry.Unauthorized();
        }

        if (operator == address(0)) {
            if (state.committeeMemberCounts[e3Id] != 0) {
                revert IBondingRegistry.InvalidConfiguration();
            }
            delete state.committeeRegistries[e3Id];
            state.activeCommitteeAssignments--;
        } else if (state.committeeObligations[e3Id][operator]) {
            delete state.committeeObligations[e3Id][operator];
            state.committeeMemberCounts[e3Id]--;
            state.unresolvedCommittees[operator]--;
        }
    }

    function snapshotE3(
        address manager,
        uint256 e3Id,
        address refundManager,
        address interfold,
        mapping(address => mapping(uint256 => address)) storage destinations
    ) external {
        require(
            refundManager != address(0) && interfold != address(0),
            IBondingRegistry.ZeroAddress()
        );
        if (destinations[manager][e3Id] != address(0)) {
            revert IBondingRegistry.InvalidConfiguration();
        }

        BondingSlashingStorage.Layout storage state = _layout();
        destinations[manager][e3Id] = refundManager;
        state.e3Interfold[manager][e3Id] = interfold;
        state.managers[manager].e3Assignments++;
        emit IBondingRegistry.SlashRouteDestinationSnapshotted(
            manager,
            e3Id,
            refundManager
        );
    }

    function releaseE3(
        address manager,
        uint256 e3Id,
        mapping(address => mapping(uint256 => address)) storage destinations
    ) external {
        BondingSlashingStorage.Layout storage state = _layout();
        address interfold = state.e3Interfold[manager][e3Id];
        if (interfold == address(0)) {
            revert IBondingRegistry.E3AssignmentNotFound(manager, e3Id);
        }
        if (
            state.e3Locks[manager][e3Id] != 0 ||
            state.e3Routes[manager][e3Id] != 0
        ) revert IBondingRegistry.InvalidConfiguration();

        IInterfold.E3Stage stage = IInterfold(interfold).getE3Stage(e3Id);
        if (
            stage != IInterfold.E3Stage.Complete &&
            stage != IInterfold.E3Stage.Failed
        ) revert IBondingRegistry.E3AssignmentNotTerminal(e3Id);

        delete destinations[manager][e3Id];
        delete state.e3Interfold[manager][e3Id];
        state.managers[manager].e3Assignments--;
        emit IBondingRegistry.SlashRouteDestinationReleased(manager, e3Id);
    }

    function openLock(
        address manager,
        uint256 e3Id,
        uint256 proposalId,
        address operator,
        mapping(address => mapping(uint256 => address)) storage destinations
    ) external {
        require(operator != address(0), IBondingRegistry.ZeroAddress());
        if (destinations[manager][e3Id] == address(0)) {
            revert IBondingRegistry.E3AssignmentNotFound(manager, e3Id);
        }

        BondingSlashingStorage.Layout storage state = _layout();
        if (state.slashLocks[manager][proposalId].operator != address(0)) {
            revert IBondingRegistry.SlashLockAlreadyExists(manager, proposalId);
        }
        state.slashLocks[manager][proposalId] = BondingSlashLock(
            e3Id,
            operator
        );
        state.openSlashLocks[operator]++;
        state.managers[manager].openSlashLocks++;
        state.e3Locks[manager][e3Id]++;
        emit IBondingRegistry.SlashLockUpdated(
            manager,
            proposalId,
            operator,
            true
        );
    }

    function closeLock(
        address manager,
        uint256 proposalId,
        address operator
    ) external {
        BondingSlashingStorage.Layout storage state = _layout();
        BondingSlashLock memory lock = state.slashLocks[manager][proposalId];
        if (lock.operator != operator || operator == address(0)) {
            revert IBondingRegistry.SlashLockNotFound(manager, proposalId);
        }

        delete state.slashLocks[manager][proposalId];
        state.openSlashLocks[operator]--;
        state.managers[manager].openSlashLocks--;
        state.e3Locks[manager][lock.e3Id]--;
        emit IBondingRegistry.SlashLockUpdated(
            manager,
            proposalId,
            operator,
            false
        );
    }

    function setBan(
        address manager,
        address operator,
        bool banned
    ) external returns (bool changed) {
        require(operator != address(0), IBondingRegistry.ZeroAddress());
        BondingSlashingStorage.Layout storage state = _layout();
        if (state.managerBans[manager][operator] == banned) return false;

        state.managerBans[manager][operator] = banned;
        if (banned) {
            state.activeBans[operator]++;
            state.managers[manager].activeBans++;
        } else {
            state.activeBans[operator]--;
            state.managers[manager].activeBans--;
        }
        emit IBondingRegistry.ManagerBanUpdated(manager, operator, banned);
        return true;
    }

    function updateRouteCount(
        address manager,
        uint256 e3Id,
        bool increase
    ) external {
        if (increase) _layout().e3Routes[manager][e3Id]++;
        else _layout().e3Routes[manager][e3Id]--;
    }

    function validateRegistryMigration(
        address currentRegistry,
        address nextRegistry,
        address caller,
        address contractOwner,
        address manager,
        address[] storage managers
    ) external view {
        if (nextRegistry.code.length == 0) {
            revert IBondingRegistry.RegistryDependencyMismatch(nextRegistry);
        }
        if (currentRegistry == address(0)) {
            if (caller != contractOwner) revert IBondingRegistry.Unauthorized();
            return;
        }
        IInterfoldMigrationView interfold = IInterfoldMigrationView(caller);
        if (interfold.isFeeTokenAllowed(interfold.feeToken())) {
            revert IInterfold.RegistryMigrationRequiresRequestPause();
        }
        _validateRegistryDependencies(
            currentRegistry,
            nextRegistry,
            caller,
            manager
        );
        _validateRegistryDrain(managers);
        _validateRegistryMembership(currentRegistry, nextRegistry);
    }

    function _validateRegistryDependencies(
        address currentRegistry,
        address nextRegistry,
        address caller,
        address manager
    ) private view {
        IRegistryMigrationView next = IRegistryMigrationView(nextRegistry);
        if (
            nextRegistry == currentRegistry ||
            IRegistryMigrationView(currentRegistry).interfold() != caller ||
            next.interfold() != caller ||
            next.getBondingRegistry() != address(this) ||
            next.slashingManager() != manager ||
            next.dkgFoldAttestationVerifier() == address(0) ||
            address(ISlashingManager(manager).ciphernodeRegistry()) !=
            nextRegistry
        ) {
            revert IBondingRegistry.RegistryDependencyMismatch(nextRegistry);
        }
    }

    function _validateRegistryDrain(address[] storage managers) private view {
        BondingSlashingStorage.Layout storage state = _layout();
        uint256 activeCommittees = state.activeCommitteeAssignments;
        if (activeCommittees != 0) {
            revert IBondingRegistry.RegistryHasActiveCommittees(
                activeCommittees
            );
        }
        for (uint256 i = 0; i < managers.length; ++i) {
            address candidate = managers[i];
            uint256 assignments = state.managers[candidate].e3Assignments;
            if (assignments != 0) {
                revert IBondingRegistry.ManagerHasE3Assignments(
                    candidate,
                    assignments
                );
            }
        }
    }

    function _validateRegistryMembership(
        address currentRegistry,
        address nextRegistry
    ) private view {
        uint256 expectedCount = IRegistryMigrationView(currentRegistry)
            .numCiphernodes();
        uint256 actualCount = IRegistryMigrationView(nextRegistry)
            .numCiphernodes();
        uint256 expectedRoot = IRegistryMigrationView(currentRegistry).root();
        uint256 actualRoot = IRegistryMigrationView(nextRegistry).root();
        if (actualCount != expectedCount || actualRoot != expectedRoot) {
            revert IBondingRegistry.RegistryMembershipMismatch(
                expectedCount,
                actualCount,
                expectedRoot,
                actualRoot
            );
        }
    }

    function authorizeManager(
        address manager,
        address bonding,
        uint256 maxManagers,
        address[] storage managers,
        mapping(address => uint256) storage indexPlusOne
    ) external {
        _validateManager(manager, bonding);
        if (indexPlusOne[manager] != 0) return;
        if (managers.length >= maxManagers) {
            revert IBondingRegistry.InvalidConfiguration();
        }
        managers.push(manager);
        indexPlusOne[manager] = managers.length;
        emit IBondingRegistry.SlashingManagerAuthorizationUpdated(
            manager,
            true
        );
    }

    function revokeManager(
        address manager,
        address currentManager,
        address[] storage managers,
        mapping(address => uint256) storage indexPlusOne,
        mapping(address => uint256) storage pendingRoutes
    ) external {
        if (manager == currentManager) {
            revert IBondingRegistry.InvalidConfiguration();
        }
        uint256 index = indexPlusOne[manager];
        if (index == 0) revert IBondingRegistry.Unauthorized();

        uint256 routes = pendingRoutes[manager];
        if (routes != 0) {
            revert IBondingRegistry.ManagerHasPendingSlashRoutes(
                manager,
                routes
            );
        }
        SlashingManagerObligations storage obligations = _layout().managers[
            manager
        ];
        if (obligations.openSlashLocks != 0) {
            revert IBondingRegistry.ManagerHasOpenSlashLocks(
                manager,
                obligations.openSlashLocks
            );
        }
        if (obligations.activeBans != 0) {
            revert IBondingRegistry.ManagerHasActiveBans(
                manager,
                obligations.activeBans
            );
        }
        if (obligations.e3Assignments != 0) {
            revert IBondingRegistry.ManagerHasE3Assignments(
                manager,
                obligations.e3Assignments
            );
        }

        uint256 lastIndex = managers.length;
        if (index != lastIndex) {
            address moved = managers[lastIndex - 1];
            managers[index - 1] = moved;
            indexPlusOne[moved] = index;
        }
        managers.pop();
        delete indexPlusOne[manager];
        emit IBondingRegistry.SlashingManagerAuthorizationUpdated(
            manager,
            false
        );
    }

    function _validateManager(address manager, address bonding) private view {
        if (manager.code.length == 0) {
            revert IBondingRegistry.IncompatibleSlashingManager(manager);
        }
        if (
            _probe(
                manager,
                abi.encodeCall(
                    ISlashingManager.SLASHING_MANAGER_API_VERSION,
                    ()
                )
            ) != API_VERSION
        ) revert IBondingRegistry.IncompatibleSlashingManager(manager);

        uint256 configuredRegistry = _probe(
            manager,
            abi.encodeCall(ISlashingManager.bondingRegistry, ())
        );
        if (configuredRegistry > type(uint160).max) {
            revert IBondingRegistry.IncompatibleSlashingManager(manager);
        }
        address configured = address(uint160(configuredRegistry));
        if (configured != bonding) {
            revert IBondingRegistry.SlashingManagerBondingMismatch(
                manager,
                configured
            );
        }

        if (
            _probe(
                manager,
                abi.encodeCall(
                    IERC165.supportsInterface,
                    (type(ISlashingManager).interfaceId)
                )
            ) != 1
        ) revert IBondingRegistry.IncompatibleSlashingManager(manager);
    }

    function _probe(
        address manager,
        bytes memory callData
    ) private view returns (uint256 value) {
        (bool success, bytes memory result) = manager.staticcall{
            gas: PROBE_GAS
        }(callData);
        if (!success || result.length != 32) {
            revert IBondingRegistry.IncompatibleSlashingManager(manager);
        }
        // solhint-disable-next-line no-inline-assembly
        assembly ("memory-safe") {
            value := mload(add(result, 0x20))
        }
    }

    function _layout()
        private
        pure
        returns (BondingSlashingStorage.Layout storage state)
    {
        bytes32 slot = BONDING_SLASHING_STORAGE_SLOT;
        // solhint-disable-next-line no-inline-assembly
        assembly ("memory-safe") {
            state.slot := slot
        }
    }
}
