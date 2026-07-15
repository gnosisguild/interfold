// SPDX-License-Identifier: LGPL-3.0-only
pragma solidity 0.8.28;

import { IInterfold } from "../interfaces/IInterfold.sol";
import { InterfoldPricing } from "../lib/InterfoldPricing.sol";

/// @dev Pins sentinels immediately before and after the pricing struct so the
///      assembly-backed initializer can be tested against its documented slots.
contract InterfoldPricingStorageHarness {
    uint256[23] private _prefix;
    bytes32 public leftSentinel;
    IInterfold.PricingConfig private _pricingConfig;
    bytes32 public rightSentinel;

    constructor(bytes32 left, bytes32 right) {
        leftSentinel = left;
        rightSentinel = right;
    }

    function dirtyPricing() external {
        _pricingConfig = IInterfold.PricingConfig({
            keyGenFixedPerNode: type(uint256).max,
            keyGenPerEncryptionProof: type(uint256).max,
            coordinationPerPair: type(uint256).max,
            availabilityPerNodePerSec: type(uint256).max,
            decryptionPerNode: type(uint256).max,
            publicationBase: type(uint256).max,
            verificationPerProof: type(uint256).max,
            protocolTreasury: address(type(uint160).max),
            marginBps: type(uint16).max,
            protocolShareBps: type(uint16).max,
            dkgUtilizationBps: type(uint16).max,
            computeUtilizationBps: type(uint16).max,
            decryptUtilizationBps: type(uint16).max,
            minCommitteeSize: type(uint32).max,
            minThreshold: type(uint32).max
        });
    }

    function applyDefaultPricingConfig() external {
        InterfoldPricing.applyDefaultPricingConfig();
    }

    function getPricingConfig()
        external
        view
        returns (IInterfold.PricingConfig memory)
    {
        return _pricingConfig;
    }
}
