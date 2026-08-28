// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import {
    VRFConsumerBaseV2Plus
} from "@chainlink/contracts/src/v0.8/vrf/dev/VRFConsumerBaseV2Plus.sol";
import {
    VRFV2PlusClient
} from "@chainlink/contracts/src/v0.8/vrf/dev/libraries/VRFV2PlusClient.sol";
import { IRandomnessProvider } from "../interfaces/IRandomnessProvider.sol";
import { RegistrySortitionLib } from "../lib/RegistrySortitionLib.sol";

/// @title ChainlinkVrfRandomnessProvider
/// @notice Adapts one Chainlink VRF v2.5 subscription to one ciphernode registry.
/// @dev The callback only records the response. Registry state changes happen in normal calls,
///      so a registry failure cannot consume an unrecorded VRF response.
contract ChainlinkVrfRandomnessProvider is
    IRandomnessProvider,
    VRFConsumerBaseV2Plus
{
    struct RandomnessResult {
        uint256 randomWord;
        uint256 fulfilledAt;
        uint256 fulfilledBlock;
        bool exists;
        bool fulfilled;
    }

    error OnlyRequester(address caller);
    error InvalidRequester(address requesterAddress);
    error InvalidProtocolOwner(address protocolOwner);
    error InvalidSubscriptionId();
    error InvalidKeyHash();
    error InvalidRequestConfirmations();
    error InvalidCallbackGasLimit();
    error InvalidMinimumSubscriptionBalance();
    error UnsupportedChain(uint256 chainId);
    error InsufficientSubscriptionBalance(
        uint96 availableBalance,
        uint96 minimumBalance
    );
    error RandomnessAlreadyRequested(uint256 e3Id);

    event RandomnessResponseIgnored(uint256 indexed requestId);

    address public immutable override requester; // solhint-disable-line immutable-vars-naming
    uint256 public immutable subscriptionId; // solhint-disable-line immutable-vars-naming
    bytes32 public immutable keyHash; // solhint-disable-line immutable-vars-naming
    uint16 public immutable requestConfirmations; // solhint-disable-line immutable-vars-naming
    uint32 public immutable callbackGasLimit; // solhint-disable-line immutable-vars-naming
    bool public immutable nativePayment; // solhint-disable-line immutable-vars-naming
    uint96 public immutable minimumSubscriptionBalance; // solhint-disable-line immutable-vars-naming

    mapping(uint256 e3Id => bool requested) public randomnessRequested;
    mapping(uint256 e3Id => uint256 requestId) public requestIdByE3Id;
    mapping(uint256 requestId => uint256 e3Id) public e3IdByRequestId;
    mapping(uint256 requestId => RandomnessResult result) private _results;

    constructor(
        address requesterAddress,
        address coordinator,
        uint256 vrfSubscriptionId,
        bytes32 vrfKeyHash,
        uint16 vrfRequestConfirmations,
        uint32 vrfCallbackGasLimit,
        bool payInNativeToken,
        uint96 vrfMinimumSubscriptionBalance,
        address protocolOwner
    ) VRFConsumerBaseV2Plus(coordinator) {
        _requireSupportedChain(block.chainid);
        _validateAddresses(requesterAddress, protocolOwner);
        _validateVrfConfiguration(
            vrfSubscriptionId,
            vrfKeyHash,
            vrfRequestConfirmations,
            vrfCallbackGasLimit,
            vrfMinimumSubscriptionBalance
        );

        requester = requesterAddress;
        subscriptionId = vrfSubscriptionId;
        keyHash = vrfKeyHash;
        requestConfirmations = vrfRequestConfirmations;
        callbackGasLimit = vrfCallbackGasLimit;
        nativePayment = payInNativeToken;
        minimumSubscriptionBalance = vrfMinimumSubscriptionBalance;

        if (protocolOwner != msg.sender) transferOwnership(protocolOwner);
    }

    /// @inheritdoc IRandomnessProvider
    function requestRandomness(
        uint256 e3Id
    ) external returns (uint256 requestId) {
        if (msg.sender != requester) revert OnlyRequester(msg.sender);
        if (randomnessRequested[e3Id]) revert RandomnessAlreadyRequested(e3Id);

        (uint96 linkBalance, uint96 nativeBalance, , , ) = s_vrfCoordinator
            .getSubscription(subscriptionId);
        uint96 availableBalance = nativePayment ? nativeBalance : linkBalance;
        if (availableBalance < minimumSubscriptionBalance) {
            revert InsufficientSubscriptionBalance(
                availableBalance,
                minimumSubscriptionBalance
            );
        }

        // Set this before the external call so one E3 can never request twice.
        randomnessRequested[e3Id] = true;
        requestId = s_vrfCoordinator.requestRandomWords(
            VRFV2PlusClient.RandomWordsRequest({
                keyHash: keyHash,
                subId: subscriptionId,
                requestConfirmations: requestConfirmations,
                callbackGasLimit: callbackGasLimit,
                numWords: 1,
                extraArgs: VRFV2PlusClient._argsToBytes(
                    VRFV2PlusClient.ExtraArgsV1({
                        nativePayment: nativePayment
                    })
                )
            })
        );

        requestIdByE3Id[e3Id] = requestId;
        e3IdByRequestId[requestId] = e3Id;
        _results[requestId].exists = true;
        emit RandomnessRequested(requestId, e3Id);
    }

    /// @inheritdoc IRandomnessProvider
    function getRandomness(
        uint256 requestId
    )
        external
        view
        returns (
            bool fulfilled,
            uint256 randomWord,
            uint256 fulfilledAt,
            uint256 fulfilledBlock
        )
    {
        RandomnessResult storage result = _results[requestId];
        return (
            result.fulfilled,
            result.randomWord,
            result.fulfilledAt,
            result.fulfilledBlock
        );
    }

    /// @dev Never reverts for an unknown, duplicate, or malformed response. Chainlink does not
    ///      retry a callback that reverts.
    function fulfillRandomWords(
        uint256 requestId,
        uint256[] calldata randomWords
    ) internal override {
        RandomnessResult storage result = _results[requestId];
        if (!result.exists || result.fulfilled || randomWords.length != 1) {
            emit RandomnessResponseIgnored(requestId);
            return;
        }

        result.randomWord = randomWords[0];
        result.fulfilledAt = block.timestamp;
        result.fulfilledBlock = RegistrySortitionLib.currentBlockNumber();
        result.fulfilled = true;
        emit RandomnessFulfilled(
            requestId,
            e3IdByRequestId[requestId],
            randomWords[0],
            block.timestamp
        );
    }

    function _requireSupportedChain(uint256 chainId) private pure {
        bool supported = chainId == 1 ||
            chainId == 11_155_111 ||
            chainId == 31_337 ||
            chainId == 1_337;
        if (!supported) revert UnsupportedChain(chainId);
    }

    function _validateAddresses(
        address requesterAddress,
        address protocolOwner
    ) private view {
        if (requesterAddress == address(0) || requesterAddress.code.length == 0)
            revert InvalidRequester(requesterAddress);
        if (protocolOwner == address(0))
            revert InvalidProtocolOwner(protocolOwner);
    }

    function _validateVrfConfiguration(
        uint256 vrfSubscriptionId,
        bytes32 vrfKeyHash,
        uint16 vrfRequestConfirmations,
        uint32 vrfCallbackGasLimit,
        uint96 vrfMinimumSubscriptionBalance
    ) private pure {
        if (vrfSubscriptionId == 0) revert InvalidSubscriptionId();
        if (vrfKeyHash == bytes32(0)) revert InvalidKeyHash();
        if (vrfRequestConfirmations == 0) revert InvalidRequestConfirmations();
        if (vrfCallbackGasLimit == 0) revert InvalidCallbackGasLimit();
        if (vrfMinimumSubscriptionBalance == 0)
            revert InvalidMinimumSubscriptionBalance();
    }
}
