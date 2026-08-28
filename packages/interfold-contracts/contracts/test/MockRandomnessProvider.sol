// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

import { IRandomnessProvider } from "../interfaces/IRandomnessProvider.sol";
import { RegistrySortitionLib } from "../lib/RegistrySortitionLib.sol";

/// @notice Controllable asynchronous randomness source for tests.
contract MockRandomnessProvider is IRandomnessProvider {
    struct Result {
        uint256 randomWord;
        uint256 fulfilledAt;
        uint256 fulfilledBlock;
        bool fulfilled;
    }

    error OnlyRequester(address caller);
    error RandomnessAlreadyFulfilled(uint256 requestId);
    error UnknownRandomnessRequest(uint256 requestId);

    address public immutable override requester;
    /// @dev Unit-test shortcut only. Local integration tests must call `fulfill` in a later
    /// transaction so off-chain readers observe the same block boundary as production VRF.
    bool public autoFulfill;
    bool public autoFulfillInRequestBlock;
    uint256 public nextRequestId = 1;
    mapping(uint256 e3Id => uint256 requestId) public requestIdByE3Id;
    mapping(uint256 requestId => uint256 e3Id) public e3IdByRequestId;
    mapping(uint256 requestId => bool known) private _knownRequests;
    mapping(uint256 requestId => Result result) private _results;

    constructor(address requesterAddress) {
        requester = requesterAddress;
    }

    function setAutoFulfill(bool enabled) external {
        autoFulfill = enabled;
    }

    function setAutoFulfillInRequestBlock(bool enabled) external {
        autoFulfillInRequestBlock = enabled;
    }

    function requestRandomness(
        uint256 e3Id
    ) external returns (uint256 requestId) {
        if (msg.sender != requester) revert OnlyRequester(msg.sender);
        requestId = nextRequestId++;
        requestIdByE3Id[e3Id] = requestId;
        e3IdByRequestId[requestId] = e3Id;
        _knownRequests[requestId] = true;
        emit RandomnessRequested(requestId, e3Id);
        if (autoFulfill) {
            uint256 currentBlock = RegistrySortitionLib.currentBlockNumber();
            _fulfill(
                requestId,
                uint256(keccak256(abi.encode(e3Id, requestId))),
                autoFulfillInRequestBlock
                    ? block.timestamp
                    : block.timestamp + 1,
                autoFulfillInRequestBlock ? currentBlock : currentBlock + 1
            );
        }
    }

    function fulfill(uint256 requestId, uint256 randomWord) external {
        _fulfill(
            requestId,
            randomWord,
            block.timestamp,
            RegistrySortitionLib.currentBlockNumber()
        );
    }

    function fulfillAt(
        uint256 requestId,
        uint256 randomWord,
        uint256 fulfilledAt
    ) external {
        _fulfill(
            requestId,
            randomWord,
            fulfilledAt,
            RegistrySortitionLib.currentBlockNumber()
        );
    }

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
        Result storage result = _results[requestId];
        return (
            result.fulfilled,
            result.randomWord,
            result.fulfilledAt,
            result.fulfilledBlock
        );
    }

    function _fulfill(
        uint256 requestId,
        uint256 randomWord,
        uint256 fulfilledAt,
        uint256 fulfilledBlock
    ) private {
        if (!_knownRequests[requestId])
            revert UnknownRandomnessRequest(requestId);
        if (_results[requestId].fulfilled)
            revert RandomnessAlreadyFulfilled(requestId);
        _results[requestId] = Result({
            randomWord: randomWord,
            fulfilledAt: fulfilledAt,
            fulfilledBlock: fulfilledBlock,
            fulfilled: true
        });
        emit RandomnessFulfilled(
            requestId,
            e3IdByRequestId[requestId],
            randomWord,
            fulfilledAt
        );
    }
}
