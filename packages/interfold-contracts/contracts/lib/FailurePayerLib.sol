// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IInterfold } from "../interfaces/IInterfold.sol";
import { IE3RefundManager } from "../interfaces/IE3RefundManager.sol";

/// @notice Defines which party pays for each E3 failure reason.
library FailurePayerLib {
    function getFailurePayer(
        IInterfold.FailureReason reason
    ) internal pure returns (IE3RefundManager.FailurePayer payer) {
        if (
            reason == IInterfold.FailureReason.NoInputsReceived ||
            reason == IInterfold.FailureReason.ComputeTimeout ||
            reason == IInterfold.FailureReason.ComputeProviderExpired ||
            reason == IInterfold.FailureReason.ComputeProviderFailed ||
            reason == IInterfold.FailureReason.RequesterCancelled
        ) {
            return IE3RefundManager.FailurePayer.Requester;
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
            return IE3RefundManager.FailurePayer.Ciphernodes;
        }

        revert IE3RefundManager.InvalidFailureReason(reason);
    }
}
