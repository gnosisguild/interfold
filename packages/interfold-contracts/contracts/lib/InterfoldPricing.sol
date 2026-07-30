// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

import { IInterfold } from "../interfaces/IInterfold.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

/**
 * @title InterfoldPricing
 * @notice External library extracted from {Interfold} to keep its deployed
 *         runtime bytecode under the EIP-170 24,576-byte cap.
 *
 *         Functions contain fee-quote math, pricing validation, and bounded
 *         reward accounting. External calls use DELEGATECALL to keep this code
 *         out of the Interfold runtime bytecode.
 */
library InterfoldPricing {
    uint16 internal constant BPS_BASE = 10000;
    event RewardCredited(
        uint256 indexed e3Id,
        address indexed account,
        IERC20 indexed token,
        uint256 amount
    );

    /// @notice Mirrors the threshold / min-size gates at the top of
    ///         {Interfold.getE3Quote} (post param-set existence check).
    /// @param committeeSize  ABI-encoded as `uint8` to avoid qualified enum
    ///                       names in the library ABI (ethers v6 rejects
    ///                       `IInterfold.CommitteeSize`).
    function validateQuoteThresholds(
        uint32[2] memory threshold,
        uint8 committeeSize,
        uint32 minCommitteeSize,
        uint32 minThreshold
    ) external pure {
        IInterfold.CommitteeSize size = IInterfold.CommitteeSize(committeeSize);
        if (threshold[1] == 0)
            revert IInterfold.CommitteeSizeNotConfigured(size);
        if (minCommitteeSize > 0 && threshold[1] < minCommitteeSize)
            revert IInterfold.CommitteeSizeTooSmall(size);
        if (minThreshold > 0 && threshold[0] < minThreshold)
            revert IInterfold.ThresholdTooSmall(threshold[0]);
    }

    /// @notice Mirrors {Interfold.setPricingConfig} validation.
    function validatePricingConfig(
        IInterfold.PricingConfig calldata config,
        uint16 maxMarginBps,
        uint16 maxProtocolShareBps
    ) external pure {
        if (config.marginBps > maxMarginBps)
            revert IInterfold.BpsExceedsMax(config.marginBps);
        if (config.protocolShareBps > maxProtocolShareBps)
            revert IInterfold.BpsExceedsMax(config.protocolShareBps);
        if (config.dkgUtilizationBps > BPS_BASE)
            revert IInterfold.UtilizationBpsExceedsMax(
                config.dkgUtilizationBps
            );
        if (config.computeUtilizationBps > BPS_BASE)
            revert IInterfold.UtilizationBpsExceedsMax(
                config.computeUtilizationBps
            );
        if (config.decryptUtilizationBps > BPS_BASE)
            revert IInterfold.UtilizationBpsExceedsMax(
                config.decryptUtilizationBps
            );
        if (
            config.protocolShareBps != 0 &&
            config.protocolTreasury == address(0)
        ) revert IInterfold.TreasuryRequired();
        if (config.minCommitteeSize < config.minThreshold)
            revert IInterfold.MinSizeBelowMinThreshold();
    }

    /// @notice Splits and credits committee rewards to each operator's bond
    ///         owner, sweeping integer-division dust into a slot selected by
    ///         `e3Id % n`.
    /// @dev Runs through a linked library call to keep the accounting loop out
    ///      of Interfold's size-constrained runtime bytecode.
    function computeAndCreditRewards(
        mapping(uint256 => mapping(address => uint256)) storage pendingRewards,
        IBondingRegistry bonding,
        uint256 cnAmount,
        uint256 e3Id,
        address[] memory nodes,
        IERC20 token
    ) external returns (uint256[] memory amounts) {
        uint256 n = nodes.length;
        amounts = new uint256[](n);
        uint256 per = cnAmount / n;
        uint256 dust = cnAmount - per * n;
        uint256 dustIndex = e3Id % n;
        for (uint256 i = 0; i < n; i++) {
            uint256 amount = per;
            if (i == dustIndex) amount += dust;
            amounts[i] = amount;
            if (amount > 0) {
                address operator = nodes[i];
                address recipient = bonding.bondOwnerOf(operator);
                if (recipient == address(0)) recipient = operator;
                pendingRewards[e3Id][recipient] += amount;
                emit RewardCredited(e3Id, recipient, token, amount);
            }
        }
    }

    /// @notice Pure fee quote math. The caller (Interfold) is responsible for
    ///         loading the per-call inputs and gating on min-committee / min-
    ///         threshold (so we keep the original {CommitteeSize} discriminator
    ///         in revert data).
    /// @param pc                  Snapshot of `_pricingConfig`.
    /// @param tc                  Snapshot of `_timeoutConfig`.
    /// @param sortitionWindow     Result of `ciphernodeRegistry.sortitionSubmissionWindow()`.
    /// @param threshold           `[quorum, total]` resolved from `committeeThresholds`.
    /// @param inputWindowStart    `requestParams.inputWindow[0]`.
    /// @param inputWindowEnd      `requestParams.inputWindow[1]`.
    function quote(
        IInterfold.PricingConfig calldata pc,
        IInterfold.E3TimeoutConfig calldata tc,
        uint256 sortitionWindow,
        uint32[2] calldata threshold,
        uint256 inputWindowStart,
        uint256 inputWindowEnd
    ) external view returns (uint256 fee) {
        if (inputWindowEnd < inputWindowStart)
            revert IInterfold.InvalidInputDeadlineEnd(inputWindowEnd);

        {
            uint256 computeDeadline = inputWindowEnd + tc.computeWindow;
            uint256 committeeDeadline = block.timestamp + sortitionWindow;
            if (computeDeadline <= committeeDeadline)
                revert IInterfold.ComputeDeadlinePrecedesCommitteeFinalization(
                    computeDeadline,
                    committeeDeadline
                );
        }

        uint256 n = uint256(threshold[1]); // total committee size
        uint256 m = uint256(threshold[0]); // quorum/decryption threshold

        // Duration covers the full availability period, using expected-case
        // utilization fractions for protocol-controlled timeout windows.
        // Sum the BPS-weighted windows first then divide once so the
        // duration does not lose up to ~3 seconds of weight to per-term
        // integer-division truncation.
        uint256 weightedTimeoutsBps = tc.dkgWindow *
            uint256(pc.dkgUtilizationBps) +
            tc.computeWindow *
            uint256(pc.computeUtilizationBps) +
            tc.decryptionWindow *
            uint256(pc.decryptUtilizationBps);
        uint256 duration = sortitionWindow +
            inputWindowEnd -
            inputWindowStart +
            weightedTimeoutsBps /
            uint256(BPS_BASE);

        // ZK proof count per node: 14 fixed + 4 × (N-1) scaling.
        uint256 proofsPerNode = 14 + 4 * (n - 1);

        // Key generation cost: fixed per-node + per-proof (quadratic in n)
        uint256 baseFee = pc.keyGenFixedPerNode * n;
        baseFee += pc.keyGenPerEncryptionProof * n * proofsPerNode;

        // Key generation coordination cost (quadratic in n)
        if (n > 1) {
            baseFee += (pc.coordinationPerPair * (n * (n - 1))) / 2;
        }

        // Proof verification cost: each node verifies all others' proofs.
        baseFee += pc.verificationPerProof * n * proofsPerNode;

        // Availability cost (linear in n × duration)
        baseFee += pc.availabilityPerNodePerSec * n * duration;

        // Decryption cost (linear in m)
        baseFee += pc.decryptionPerNode * m;
        // Decryption coordination cost (quadratic in m)
        if (m > 1) {
            baseFee += (pc.coordinationPerPair * (m * (m - 1))) / 2;
        }

        // Publication base cost
        baseFee += pc.publicationBase;

        // Apply margin markup
        fee =
            (baseFee * (uint256(BPS_BASE) + uint256(pc.marginBps))) /
            uint256(BPS_BASE);

        if (fee == 0) revert IInterfold.PaymentRequired(fee);
    }
}
