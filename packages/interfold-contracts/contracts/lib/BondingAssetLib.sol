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

    function validateTicketToken(
        address currentAddress,
        address nextAddress
    ) external view {
        if (nextAddress.code.length == 0) {
            revert IBondingRegistry.InvalidBondingAsset(nextAddress);
        }
        if (currentAddress == address(0) || currentAddress == nextAddress) {
            return;
        }

        InterfoldTicketToken current = InterfoldTicketToken(currentAddress);
        uint256 liabilities = current.totalSupply() + current.payableBalance();
        if (liabilities != 0) {
            revert IBondingRegistry.OutstandingAssetLiabilities(
                currentAddress,
                liabilities
            );
        }
    }

    function validateLicenseToken(
        address currentAddress,
        address nextAddress,
        address registry
    ) external view {
        if (nextAddress == address(0)) {
            if (currentAddress != address(0)) {
                revert IBondingRegistry.InvalidBondingAsset(nextAddress);
            }
            return;
        }
        if (nextAddress.code.length == 0) {
            revert IBondingRegistry.InvalidBondingAsset(nextAddress);
        }

        if (currentAddress != address(0) && currentAddress != nextAddress) {
            uint256 liabilities = IERC20(currentAddress).balanceOf(registry);
            if (liabilities != 0) {
                revert IBondingRegistry.OutstandingAssetLiabilities(
                    currentAddress,
                    liabilities
                );
            }
        }
        lockedBalanceOf(nextAddress, registry);
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
        uint256 received = token.balanceOf(recipient) - beforeBalance;
        if (received != amount) {
            emit IBondingRegistry.LicenseTransferShortfall(
                recipient,
                amount,
                received
            );
        }
    }
}
