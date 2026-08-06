// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity 0.8.28;

contract MockRisc0ComputeVerifier {
    error UnexpectedJournalDigest(bytes32 actual, bytes32 expected);

    bytes32 public expectedJournalDigest;

    function setExpectedJournalDigest(bytes32 value) external {
        expectedJournalDigest = value;
    }

    function verify(
        bytes calldata,
        bytes32,
        bytes32 journalDigest
    ) external view {
        if (journalDigest != expectedJournalDigest)
            revert UnexpectedJournalDigest(
                journalDigest,
                expectedJournalDigest
            );
    }
}
