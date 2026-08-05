// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

import { IRiscZeroVerifier } from "risc0/IRiscZeroVerifier.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IE3Program } from "@interfold/contracts/contracts/interfaces/IE3Program.sol";
import { IInterfold } from "@interfold/contracts/contracts/interfaces/IInterfold.sol";
import { E3 } from "@interfold/contracts/contracts/interfaces/IE3.sol";
import { Risc0ComputeProof } from "@interfold/contracts/contracts/lib/Risc0ComputeProof.sol";
import { LazyIMTData, InternalLazyIMT } from "@zk-kit/lazy-imt.sol/InternalLazyIMT.sol";
import { HonkVerifier } from "./CRISPVerifier.sol";

contract CRISPProgram is IE3Program, Ownable {
  using InternalLazyIMT for LazyIMTData;

  /// @notice Enum to represent credit modes
  enum CreditMode {
    /// @notice Everyone has constant credits
    CONSTANT,
    /// @notice Credits are custom (can be based on token balance, etc)
    CUSTOM
  }

  /// @notice Where the eligible voter set for a round comes from.
  /// @dev Two sources with opposite economics. TOKEN derives the electorate from balances at a
  /// snapshot: the coordinator enumerates holders, which is expensive and needs an indexer, but it
  /// is the only way to answer "everyone holding this token". BY_REQUESTER asks the requesting
  /// contract, which already knows its own membership — a game roster, an allowlisted cohort, a
  /// committee — so nothing is enumerated and no indexer is involved.
  ///
  /// Declared explicitly rather than inferred. A coordinator that probed every requester and
  /// silently fell back on failure would turn a broken census provider into a token vote with the
  /// wrong electorate, and nothing would error.
  ///
  /// Required, not optional: params must carry it. Making it defaultable would mean a caller that
  /// forgot it silently got token discovery, which is the same silent-wrong-electorate failure one
  /// level up.
  enum CensusMode {
    /// @notice Derived from token balances by the coordinator. The default.
    TOKEN,
    /// @notice Supplied by the requester via `getCensus(uint256 e3Id) returns (address[])`.
    BY_REQUESTER
  }

  /// @notice Struct to store all data related to a voting round
  struct RoundData {
    uint256 merkleRoot;
    bytes32 paramsHash;
    mapping(address slot => uint40 index) voteSlots;
    LazyIMTData votes;
    uint256 numOptions;
    CreditMode creditMode;
    CensusMode censusMode;
  }

  // Constants
  /// @notice Encryption scheme ID used for the CRISP program.
  bytes32 public constant ENCRYPTION_SCHEME_ID = keccak256("fhe.rs:BFV");
  /// @notice The depth of the input Merkle tree.
  uint8 public constant TREE_DEPTH = 20;
  /// @notice Number of leading plaintext coefficients that carry the vote payload.
  /// @dev Must stay aligned with `@crisp-e3/sdk` and `crisp_utils` (`MAX_MSG_NON_ZERO_COEFFS`).
  /// The remaining coefficients up to the BFV degree are zero padding.
  uint256 constant MAX_MSG_NON_ZERO_COEFFS = 100;
  /// @notice Maximum number of vote options a round may configure.
  /// @dev Bounded by the Noir circuit, which asserts `num_options <= MAX_OPTIONS`
  /// (`circuits/lib/src/constants.nr`). A round above this accepts no ballot, because every
  /// vote proof fails. Must stay aligned with the SDK constant of the same name.
  uint256 constant MAX_VOTE_OPTIONS = 10;
  // State variables
  IInterfold public interfold;
  IRiscZeroVerifier public risc0Verifier;
  bytes32 public imageId;
  HonkVerifier private immutable honkVerifier;

  // Mappings
  mapping(uint256 e3Id => RoundData) e3Data;

  // Errors
  error CallerNotAuthorized();
  error E3AlreadyInitialized();
  error InterfoldAddressZero();
  error Risc0VerifierAddressZero();
  error InvalidHonkVerifier();
  error EmptyInputData();
  error InvalidNoirProof();
  error InvalidMerkleRoot();
  error MerkleRootAlreadySet();
  error InvalidTallyLength();
  /// @notice A requester-supplied census names who may vote, not how much each vote weighs, so it
  /// only has meaning when every voter carries the same credits.
  error CensusModeRequiresConstantCredits();
  error InvalidCensusMode();
  error SlotIsEmpty();
  error MerkleRootNotSet();
  error InvalidNumOptions();
  error InputDeadlinePassed(uint256 e3Id, uint256 deadline);
  error KeyNotPublished(uint256 e3Id);
  error E3NotAcceptingInputs(uint256 e3Id);
  error InvalidComputeContext();

  // Events
  event InputPublished(uint256 indexed e3Id, bytes encryptedVote, uint256 index);

  /// @notice Initialize the contract, binding it to a specified RISC Zero verifier.
  /// @param _interfold The interfold address
  /// @param _risc0Verifier The RISC Zero verifier address
  /// @param _honkVerifier The honk verifier address
  /// @param _imageId The image ID for the guest program
  constructor(IInterfold _interfold, IRiscZeroVerifier _risc0Verifier, HonkVerifier _honkVerifier, bytes32 _imageId) Ownable(msg.sender) {
    if (address(_interfold) == address(0)) revert InterfoldAddressZero();
    if (address(_risc0Verifier) == address(0)) revert Risc0VerifierAddressZero();
    if (address(_honkVerifier) == address(0)) revert InvalidHonkVerifier();

    interfold = _interfold;
    risc0Verifier = _risc0Verifier;
    honkVerifier = _honkVerifier;
    imageId = _imageId;
  }

  /// @notice Sets the Merkle root for an E3 program. Can only be set once.
  /// @param _e3Id The E3 program ID
  /// @param _root The Merkle root to set.
  function setMerkleRoot(uint256 _e3Id, uint256 _root) external onlyOwner {
    if (_root == 0) revert InvalidMerkleRoot();
    if (e3Data[_e3Id].merkleRoot != 0) revert MerkleRootAlreadySet();

    e3Data[_e3Id].merkleRoot = _root;
  }

  /// @notice Set the Image ID for the guest program
  /// @param _imageId The new image ID.
  function setImageId(bytes32 _imageId) external onlyOwner {
    imageId = _imageId;
  }

  /// @notice Set the RISC Zero verifier.
  /// @param _risc0Verifier The new RISC Zero verifier address
  function setRisc0Verifier(IRiscZeroVerifier _risc0Verifier) external onlyOwner {
    if (address(_risc0Verifier) == address(0)) revert Risc0VerifierAddressZero();
    risc0Verifier = _risc0Verifier;
  }

  /// @notice Get the params hash for an E3 program
  /// @param e3Id The E3 program ID
  /// @return The params hash
  function getParamsHash(uint256 e3Id) public view returns (bytes32) {
    return e3Data[e3Id].paramsHash;
  }

  /// @notice Get the details about an E3 such as the merkle root of the census
  /// @dev RoundData cannot be returned directly as it contains nested mappings
  /// @param e3Id The E3 program ID
  /// @return merkleRoot The census merkle root
  /// @return paramsHash The hash of the E3 program params
  /// @return numOptions The number of vote options
  /// @return creditMode The credit mode for the round
  /// @return inputRoot The current root of the input (votes) merkle tree
  /// @return numberOfVotes The number of leaves in the input merkle tree
  function getRoundData(
    uint256 e3Id
  )
    public
    view
    returns (uint256 merkleRoot, bytes32 paramsHash, uint256 numOptions, CreditMode creditMode, uint256 inputRoot, uint40 numberOfVotes)
  {
    RoundData storage round = e3Data[e3Id];

    merkleRoot = round.merkleRoot;
    paramsHash = round.paramsHash;
    numOptions = round.numOptions;
    creditMode = round.creditMode;
    inputRoot = round.votes._root();
    numberOfVotes = round.votes.numberOfLeaves;
  }

  /// @notice The census source a round was requested with.
  /// @dev A separate getter rather than a sixth return value on `getRoundData`, whose tuple is
  /// already consumed by the server and the SDK — widening it would break them for a field most
  /// callers do not want.
  /// @param e3Id The E3 to look up.
  /// @return The census mode recorded at validation.
  function censusModeOf(uint256 e3Id) external view returns (CensusMode) {
    return e3Data[e3Id].censusMode;
  }

  /// @inheritdoc IE3Program
  function validate(
    uint256 e3Id,
    uint256,
    bytes calldata e3ProgramParams,
    bytes calldata,
    bytes calldata customParams
  ) external returns (bytes32) {
    if (msg.sender != address(interfold) && msg.sender != owner()) revert CallerNotAuthorized();
    if (e3Data[e3Id].paramsHash != bytes32(0)) revert E3AlreadyInitialized();

    // Scoped so the decoded values do not outlive their use: `validate` is close enough to the
    // stack limit that holding all six of them alongside the parameters exceeds it.
    {
      // One decode, every field required. `censusMode` is read as a uint and range-checked rather
      // than decoded straight into the enum, so an unrecognised value gives a named error instead
      // of a bare panic.
      (, , uint256 numOptions, CreditMode creditMode, , uint256 rawCensusMode) = abi.decode(
        customParams,
        (address, uint256, uint256, CreditMode, uint256, uint256)
      );
      // The circuit asserts `num_options <= MAX_OPTIONS`, so a round configured above it accepts no
      // ballot at all. Reject at request time rather than stranding a round nobody can vote in.
      if (numOptions < 2 || numOptions > MAX_VOTE_OPTIONS) revert InvalidNumOptions();
      if (rawCensusMode > uint256(type(CensusMode).max)) revert InvalidCensusMode();

      // Rejected here rather than by the coordinator, so a combination that can never work costs
      // nothing: this reverts in the same transaction that requests the E3, before any fee is paid.
      if (CensusMode(rawCensusMode) == CensusMode.BY_REQUESTER && creditMode != CreditMode.CONSTANT) {
        revert CensusModeRequiresConstantCredits();
      }

      // we need to know the number of options for decoding the tally
      e3Data[e3Id].numOptions = numOptions;
      // we want to save the credit mode so it can be verified on chain by everyone
      e3Data[e3Id].creditMode = creditMode;
      // recorded so anyone can verify which electorate the round was requested against
      e3Data[e3Id].censusMode = CensusMode(rawCensusMode);
    }

    e3Data[e3Id].paramsHash = keccak256(e3ProgramParams);

    // Initialize the votes Merkle tree for this E3 ID.
    e3Data[e3Id].votes._init(TREE_DEPTH);

    return ENCRYPTION_SCHEME_ID;
  }

  /// @inheritdoc IE3Program
  function publishInput(uint256 e3Id, bytes memory data) external {
    E3 memory e3 = interfold.getE3(e3Id);

    // check that we are in the correct stage
    IInterfold.E3Stage stage = interfold.getE3Stage(e3Id);
    if (stage != IInterfold.E3Stage.KeyPublished) {
      revert KeyNotPublished(e3Id);
    }

    // check that we are not past the input deadline
    if (block.timestamp > e3.inputWindow[1]) {
      revert InputDeadlinePassed(e3Id, e3.inputWindow[1]);
    }

    // check that we are within the input window
    if (block.timestamp < e3.inputWindow[0]) {
      revert E3NotAcceptingInputs(e3Id);
    }

    // We need to ensure that the CRISP admin set the merkle root of the census.
    if (e3Data[e3Id].merkleRoot == 0) revert MerkleRootNotSet();

    if (data.length == 0) revert EmptyInputData();

    (bytes memory noirProof, address slotAddress, bytes32 encryptedVoteCommitment, bytes memory encryptedVote) = abi.decode(
      data,
      (bytes, address, bytes32, bytes)
    );

    (uint40 voteIndex, bytes32 previousEncryptedVoteCommitment) = _processVote(e3Id, slotAddress, encryptedVoteCommitment);

    // Set the public inputs for the proof. Order must match Noir circuit.
    bytes32[] memory noirPublicInputs = new bytes32[](7);
    noirPublicInputs[0] = previousEncryptedVoteCommitment;
    noirPublicInputs[1] = bytes32(e3Data[e3Id].merkleRoot);
    noirPublicInputs[2] = bytes32(uint256(uint160(slotAddress)));
    noirPublicInputs[3] = bytes32(uint256(previousEncryptedVoteCommitment == bytes32(0) ? 1 : 0));
    noirPublicInputs[4] = bytes32(e3Data[e3Id].numOptions);
    noirPublicInputs[5] = encryptedVoteCommitment;
    noirPublicInputs[6] = e3.committeePublicKey;

    // Check if the ciphertext was encrypted correctly
    if (!honkVerifier.verify(noirProof, noirPublicInputs)) {
      revert InvalidNoirProof();
    }

    emit InputPublished(e3Id, encryptedVote, voteIndex);
  }

  /// @notice Decode the tally from the plaintext output
  /// @param e3Id The E3 program ID
  /// @return votes - an array of vote counts for each option
  function decodeTally(uint256 e3Id) public view returns (uint256[] memory votes) {
    E3 memory e3 = interfold.getE3(e3Id);

    uint256 numOptions = e3Data[e3Id].numOptions;

    // If num optionsis not configured, return empty array to avoid decoding errors.
    // Users might be calling this function too early and there's no
    if (numOptions == 0) {
      return new uint256[](0);
    }

    uint64[] memory tally = _decodeBytesToUint64Array(e3.plaintextOutput);

    // The payload lives in the first MAX_MSG_NON_ZERO_COEFFS coefficients; the rest of
    // the polynomial is zero padding and must not be read.
    if (tally.length < MAX_MSG_NON_ZERO_COEFFS) revert InvalidTallyLength();

    uint256 segmentSize = MAX_MSG_NON_ZERO_COEFFS / numOptions;
    // More options than payload coefficients leaves nothing to decode.
    if (segmentSize == 0) return new uint256[](0);

    votes = new uint256[](numOptions);

    for (uint256 optIdx = 0; optIdx < numOptions; optIdx++) {
      uint256 segmentStart = optIdx * segmentSize;
      uint256 value = 0;

      // Each segment holds the count in binary, most significant coefficient first.
      for (uint256 i = 0; i < segmentSize; i++) {
        uint256 weight = 2 ** (segmentSize - 1 - i);
        value += uint256(tally[segmentStart + i]) * weight;
      }

      votes[optIdx] = value;
    }

    return votes;
  }

  /// @notice Get the slot index for a given E3 ID and slot address
  /// @param e3Id The E3 program ID
  /// @param slotAddress The slot address
  /// @return The slot index, or -1 if the slot is empty
  function getSlotIndex(uint256 e3Id, address slotAddress) external view returns (int40) {
    uint40 storedIndexPlusOne = e3Data[e3Id].voteSlots[slotAddress];
    return int40(storedIndexPlusOne) - 1;
  }

  /// @inheritdoc IE3Program
  function verify(
    uint256 e3Id,
    bytes32 ciphertextOutputHash,
    bytes32 ciphertextCommitment,
    bytes memory proof
  ) external view override returns (bool) {
    E3 memory e3 = interfold.getE3(e3Id);
    bytes32 paramsHash = getParamsHash(e3Id);
    bytes32 inputRoot = bytes32(e3Data[e3Id].votes._root());
    Risc0ComputeProof.Proof memory computeProof = Risc0ComputeProof.decode(proof);
    if (computeProof.paramsHash != paramsHash || computeProof.inputRoot != inputRoot) revert InvalidComputeContext();

    bytes memory journal = Risc0ComputeProof.journal(
      bytes32(block.chainid),
      bytes32(uint256(uint160(address(interfold)))),
      bytes32(e3Id),
      e3.encryptionSchemeId,
      e3.committeePublicKey,
      ciphertextOutputHash,
      ciphertextCommitment,
      paramsHash,
      inputRoot
    );

    risc0Verifier.verify(computeProof.seal, imageId, sha256(journal));
    return true;
  }

  /// @notice Process a vote: insert or update in the merkle tree depending
  /// on whether it's the first vote or an override.
  function _processVote(
    uint256 e3Id,
    address slotAddress,
    bytes32 encryptedVoteCommitment
  ) internal returns (uint40 voteIndex, bytes32 previousEncryptedVoteCommitment) {
    uint40 storedIndexPlusOne = e3Data[e3Id].voteSlots[slotAddress];

    // we treat the index 0 as not voted yet
    // any valid index will be index + 1
    if (storedIndexPlusOne == 0) {
      // FIRST VOTE
      previousEncryptedVoteCommitment = bytes32(0);
      voteIndex = e3Data[e3Id].votes.numberOfLeaves;
      e3Data[e3Id].voteSlots[slotAddress] = voteIndex + 1;
      e3Data[e3Id].votes._insert(uint256(encryptedVoteCommitment));
    } else {
      // RE-VOTE
      voteIndex = storedIndexPlusOne - 1;
      previousEncryptedVoteCommitment = bytes32(e3Data[e3Id].votes.elements[voteIndex]);
      e3Data[e3Id].votes._update(uint256(encryptedVoteCommitment), voteIndex);
    }
  }

  /// @notice Decode bytes to uint64 array
  /// @param data The bytes to decode (must be multiple of 8)
  /// @return result Array of uint64 values
  function _decodeBytesToUint64Array(bytes memory data) internal pure returns (uint64[] memory result) {
    if (data.length % 8 != 0) {
      revert InvalidTallyLength();
    }

    uint256 arrayLength = data.length / 8;
    result = new uint64[](arrayLength);

    for (uint256 i = 0; i < arrayLength; i++) {
      uint256 offset = i * 8;
      uint64 value = 0;

      // Read 8 bytes in little-endian order
      for (uint64 j = 0; j < 8; j++) {
        value |= uint64(uint8(data[offset + j])) << (j * 8);
      }

      result[i] = value;
    }

    return result;
  }
}
