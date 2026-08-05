// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pragma solidity 0.8.28;

import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {
    SafeERC20
} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import {
    ILockAwareLicenseToken
} from "../interfaces/ILockAwareLicenseToken.sol";
import {
    BONDING_SLASHING_STORAGE_SLOT,
    BondingSlashingStorage,
    SlashingManagerObligations
} from "../storage/BondingSlashingStorage.sol";
import { InterfoldTicketToken } from "../token/InterfoldTicketToken.sol";

/// @notice Keeps bonding-asset checks outside the size-constrained registry.
library BondingAssetLib {
    using SafeERC20 for IERC20;

    function availableTickets(
        address ticketTokenAddress,
        address operator,
        uint256 ticketPrice
    ) external view returns (uint256) {
        return
            InterfoldTicketToken(ticketTokenAddress).balanceOf(operator) /
            ticketPrice;
    }

    function ticketBalance(
        address ticketTokenAddress,
        address operator
    ) external view returns (uint256) {
        return InterfoldTicketToken(ticketTokenAddress).balanceOf(operator);
    }

    function ticketBalanceAt(
        address ticketTokenAddress,
        address operator,
        uint256 timepoint
    ) external view returns (uint256) {
        return
            InterfoldTicketToken(ticketTokenAddress).getPastVotes(
                operator,
                timepoint
            );
    }

    function validateBondingAssetConfig(
        address currentTicket,
        address currentLicense,
        uint8 currentTicketDecimals,
        uint8 currentLicenseDecimals,
        uint64 configurationVersion,
        address registry,
        IBondingRegistry.BondingAssetConfig calldata config,
        address[] storage managers,
        mapping(address => uint256) storage pendingRoutes
    ) external returns (bool assetChanged) {
        if (config.ticketPrice == 0 || config.licenseRequiredBond == 0) {
            revert IBondingRegistry.InvalidConfiguration();
        }
        _validateTicketAsset(
            currentTicket,
            currentTicketDecimals,
            config.ticketToken,
            config.expectedTicketDecimals
        );
        _validateLicenseAsset(
            currentLicense,
            currentLicenseDecimals,
            config.licenseToken,
            config.expectedLicenseDecimals,
            registry
        );

        assetChanged =
            currentTicket != config.ticketToken ||
            currentLicense != config.licenseToken ||
            currentTicketDecimals != config.expectedTicketDecimals ||
            currentLicenseDecimals != config.expectedLicenseDecimals;
        if (assetChanged) {
            _requireNoAssetConfigurationObligations(managers, pendingRoutes);
        }
        emit IBondingRegistry.BondingAssetConfigUpdated(
            InterfoldTicketToken(config.ticketToken),
            IERC20(config.licenseToken),
            config.ticketPrice,
            config.licenseRequiredBond,
            config.expectedTicketDecimals,
            config.expectedLicenseDecimals,
            configurationVersion + (assetChanged ? 1 : 0)
        );
    }

    function _validateTicketAsset(
        address current,
        uint8 currentDecimals,
        address next,
        uint8 expectedDecimals
    ) private view {
        if (next.code.length == 0) {
            revert IBondingRegistry.InvalidBondingAsset(next);
        }
        _validateDecimals(next, expectedDecimals);
        if (
            current == address(0) ||
            (current == next && currentDecimals == expectedDecimals)
        ) return;

        InterfoldTicketToken token = InterfoldTicketToken(current);
        uint256 liabilities = token.totalSupply() + token.payableBalance();
        if (liabilities != 0) {
            revert IBondingRegistry.OutstandingAssetLiabilities(
                current,
                liabilities
            );
        }
    }

    function _validateLicenseAsset(
        address current,
        uint8 currentDecimals,
        address next,
        uint8 expectedDecimals,
        address registry
    ) private view {
        if (next == address(0)) {
            if (current != address(0) || expectedDecimals != 0) {
                revert IBondingRegistry.InvalidBondingAsset(next);
            }
            return;
        }
        if (next.code.length == 0) {
            revert IBondingRegistry.InvalidBondingAsset(next);
        }
        _validateDecimals(next, expectedDecimals);
        if (
            current != address(0) &&
            (current != next || currentDecimals != expectedDecimals)
        ) {
            uint256 liabilities = IERC20(current).balanceOf(registry);
            if (liabilities != 0) {
                revert IBondingRegistry.OutstandingAssetLiabilities(
                    current,
                    liabilities
                );
            }
        }
        lockedBalanceOf(next, registry);
    }

    function _requireNoAssetConfigurationObligations(
        address[] storage managers,
        mapping(address => uint256) storage pendingRoutes
    ) private view {
        BondingSlashingStorage.Layout storage state = _slashingLayout();
        for (uint256 i = 0; i < managers.length; i++) {
            address manager = managers[i];
            SlashingManagerObligations storage obligations = state.managers[
                manager
            ];
            uint256 routes = pendingRoutes[manager];
            if (
                obligations.e3Assignments != 0 ||
                obligations.openSlashLocks != 0 ||
                routes != 0
            ) {
                revert IBondingRegistry.AssetConfigurationInUse(
                    manager,
                    obligations.e3Assignments,
                    obligations.openSlashLocks,
                    routes
                );
            }
        }
    }

    function _validateDecimals(
        address token,
        uint8 expectedDecimals
    ) private view {
        (bool success, bytes memory result) = token.staticcall(
            abi.encodeWithSignature("decimals()")
        );
        if (!success || result.length != 32) {
            revert IBondingRegistry.BondingAssetDecimalsUnavailable(token);
        }
        uint256 decoded = abi.decode(result, (uint256));
        if (decoded > type(uint8).max) {
            revert IBondingRegistry.BondingAssetDecimalsUnavailable(token);
        }
        uint8 actualDecimals = uint8(decoded);
        if (actualDecimals != expectedDecimals) {
            revert IBondingRegistry.BondingAssetDecimalsMismatch(
                token,
                expectedDecimals,
                actualDecimals
            );
        }
    }

    function _slashingLayout()
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

    function lockedBalanceOf(
        address token,
        address account
    ) public view returns (uint256) {
        (bool success, bytes memory result) = token.staticcall(
            abi.encodeCall(ILockAwareLicenseToken.lockedBalanceOf, (account))
        );
        if (!success || result.length != 32) {
            revert IBondingRegistry.IncompatibleLicenseToken(token);
        }
        return abi.decode(result, (uint256));
    }

    function transferWithDeltaCheck(
        address tokenAddress,
        address recipient,
        uint256 amount
    ) external {
        IERC20 token = IERC20(tokenAddress);
        uint256 beforeBalance = token.balanceOf(recipient);
        token.safeTransfer(recipient, amount);
        uint256 afterBalance = token.balanceOf(recipient);
        uint256 received = afterBalance > beforeBalance
            ? afterBalance - beforeBalance
            : 0;
        if (received != amount) {
            emit IBondingRegistry.LicenseTransferShortfall(
                recipient,
                amount,
                received
            );
        }
    }
}
