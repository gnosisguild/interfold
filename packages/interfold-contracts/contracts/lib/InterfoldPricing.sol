// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

import { IInterfold } from "../interfaces/IInterfold.sol";
import { IBondingRegistry } from "../interfaces/IBondingRegistry.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { ActiveCryptoConfig } from "./ActiveCryptoConfig.sol";

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
    /// @param paramSet            BFV parameter-set enum value.
    /// @param committeeSize       Committee-size enum value.
    /// @param threshold           `[H, N]` resolved from `committeeThresholds`.
    /// @param requestTime         Timestamp used for request validation and pricing.
    /// @param inputWindowStart    `requestParams.inputWindow[0]`.
    /// @param inputWindowEnd      `requestParams.inputWindow[1]`.
    function quote(
        IInterfold.PricingConfig calldata pc,
        IInterfold.E3TimeoutConfig calldata tc,
        uint256 sortitionWindow,
        uint8 paramSet,
        uint8 committeeSize,
        uint32[2] calldata threshold,
        uint256 requestTime,
        uint256 inputWindowStart,
        uint256 inputWindowEnd
    ) external pure returns (uint256 fee) {
        if (inputWindowEnd < inputWindowStart)
            revert IInterfold.InvalidInputDeadlineEnd(inputWindowEnd);

        {
            uint256 computeDeadline = inputWindowEnd + tc.computeWindow;
            uint256 committeeDeadline = requestTime + sortitionWindow;
            if (computeDeadline <= committeeDeadline)
                revert IInterfold.ComputeDeadlinePrecedesCommitteeFinalization(
                    computeDeadline,
                    committeeDeadline
                );
        }

        if (paramSet != ActiveCryptoConfig.PARAM_SET)
            revert IInterfold.UnsupportedCryptoConfig();
        IInterfold.CommitteeSize size = IInterfold.CommitteeSize(committeeSize);
        if (threshold[1] == 0)
            revert IInterfold.CommitteeSizeNotConfigured(size);
        if (pc.minCommitteeSize > 0 && threshold[1] < pc.minCommitteeSize)
            revert IInterfold.CommitteeSizeTooSmall(size);
        if (pc.minThreshold > 0 && threshold[0] < pc.minThreshold)
            revert IInterfold.ThresholdTooSmall(threshold[0]);

        ActiveCryptoConfig.validateCommittee(committeeSize, threshold);
        uint256 n = ActiveCryptoConfig.N;
        uint256 m = ActiveCryptoConfig.T;

        uint256 duration = _billableDuration(
            pc,
            tc,
            sortitionWindow,
            requestTime,
            inputWindowStart,
            inputWindowEnd
        );

        uint256 baseFee = _baseFee(pc, n, m, duration);

        // Apply margin markup
        fee =
            (baseFee * (uint256(BPS_BASE) + uint256(pc.marginBps))) /
            uint256(BPS_BASE);

        if (fee == 0) revert IInterfold.PaymentRequired(fee);
    }

    function _baseFee(
        IInterfold.PricingConfig calldata pc,
        uint256 n,
        uint256 m,
        uint256 duration
    ) private pure returns (uint256 baseFee) {
        // ZK proof count per node: 14 fixed + 4 × (N-1) scaling.
        uint256 proofsPerNode = 14 + 4 * (n - 1);

        // Key generation cost: fixed per-node + per-proof (quadratic in n)
        baseFee = pc.keyGenFixedPerNode * n;
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
    }

    function _billableDuration(
        IInterfold.PricingConfig calldata pc,
        IInterfold.E3TimeoutConfig calldata tc,
        uint256 sortitionWindow,
        uint256 requestTime,
        uint256 inputWindowStart,
        uint256 inputWindowEnd
    ) private pure returns (uint256) {
        // Charge at least the complete request-to-input-end reservation. For
        // near-term requests, preserve the existing weighted DKG estimate.
        uint256 inputWindowLength = inputWindowEnd - inputWindowStart;
        uint256 weightedPreComputeBps = (sortitionWindow + inputWindowLength) *
            BPS_BASE +
            tc.dkgWindow *
            uint256(pc.dkgUtilizationBps);
        uint256 reservedThroughInputEndBps = (inputWindowEnd - requestTime) *
            BPS_BASE;
        uint256 preComputeBps = weightedPreComputeBps >
            reservedThroughInputEndBps
            ? weightedPreComputeBps
            : reservedThroughInputEndBps;

        // Sum all weighted terms before division to avoid per-term rounding.
        uint256 durationBps = preComputeBps +
            tc.computeWindow *
            uint256(pc.computeUtilizationBps) +
            tc.decryptionWindow *
            uint256(pc.decryptUtilizationBps);
        return durationBps / uint256(BPS_BASE);
    }
}
