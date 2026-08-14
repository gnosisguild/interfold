// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { InterfoldTicketToken } from "../token/InterfoldTicketToken.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { BondingRegistry } from "../registry/BondingRegistry.sol";
import { ExitQueueLib } from "./ExitQueueLib.sol";

/**
 * @title BondingOwnershipLib
 * @notice Bond-owner assignment for `BondingRegistry`.
 *
 * @dev External, so this code is delegatecalled and does not count against the registry's
 * EIP-170 limit.
 *
 * Only the initial assignment lives here. `acceptBondOwner` stays on the registry: its lock check
 * reads through `BondingAssetLib`, which would make this a library-to-library link needing wiring
 * in every deployment path, and that check can revert — so lifting it out of the branch it
 * currently sits in would change when a transfer fails.
 */
library BondingOwnershipLib {
    using ExitQueueLib for ExitQueueLib.ExitQueueState;

    /**
     * @notice Assign the caller's bond owner.
     * @dev Reassignment is refused once the operator holds anything, because the bond owner is
     * who collateral returns to. The check covers active bond, tickets, and amounts still queued
     * for exit — an operator mid-exit still has assets to return.
     * @param bondOwners Operator to bond-owner mapping.
     * @param pendingBondOwners Proposed owners in the two-step transfer flow.
     * @param operators The registry's operator records.
     * @param exits The registry's exit queue.
     * @param ticketToken The ticket token to read balances from.
     * @param bondOwner The owner being assigned.
     */
    function setBondOwner(
        mapping(address operator => address bondOwner) storage bondOwners,
        mapping(address operator => address pendingOwner)
            storage pendingBondOwners,
        mapping(address operator => BondingRegistry.Operator data)
            storage operators,
        ExitQueueLib.ExitQueueState storage exits,
        InterfoldTicketToken ticketToken,
        address bondOwner
    ) external {
        require(bondOwner != address(0), IBondingRegistry.ZeroAddress());

        address currentOwner = bondOwners[msg.sender];
        if (currentOwner != address(0)) {
            (uint256 pendingTicket, uint256 pendingCiphernodeBond) = exits
                .getPendingAmounts(msg.sender);
            BondingRegistry.Operator storage op = operators[msg.sender];
            if (
                op.registered ||
                op.ciphernodeBond != 0 ||
                pendingCiphernodeBond != 0 ||
                ticketToken.balanceOf(msg.sender) != 0 ||
                pendingTicket != 0
            ) {
                revert IBondingRegistry.BondOwnerAlreadySet(
                    msg.sender,
                    currentOwner
                );
            }
        }

        delete pendingBondOwners[msg.sender];
        bondOwners[msg.sender] = bondOwner;

        emit IBondingRegistry.BondOwnerSet(msg.sender, bondOwner);
    }
}
