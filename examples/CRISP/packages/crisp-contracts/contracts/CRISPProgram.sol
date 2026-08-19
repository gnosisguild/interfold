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
import { EIP712 } from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import { IHonkVerifier } from "./interfaces/IHonkVerifier.sol";
import { IVotesToken } from "./interfaces/IVotesToken.sol";
import { IERC6372Clock } from "./interfaces/IERC6372Clock.sol";

interface IInterfoldProgramRegistry {
  function e3Programs(IE3Program e3Program) external view returns (bool);
}

contract CRISPProgram is IE3Program, Ownable, EIP712 {
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
    BY_REQUESTER,
    /// @notice Read from the token by this contract, one input at a time. No list is enumerated
    /// and no root is posted, so there is no census producer to trust. `publishInput` calls
    /// `getPastVotes` for the slot and passes the result to the circuit as a public input, which
    /// is why this mode uses the `crisp_onchain` verifier rather than the `crisp` one.
    ONCHAIN
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
    /// @notice The token that voting power is read from. Only used by `CensusMode.ONCHAIN`.
    address token;
    /// @notice The smallest voting power that may cast an input. Only used by
    /// `CensusMode.ONCHAIN`, where eligibility is checked per input instead of by a census.
    uint256 minVotingPower;
    /// @notice Credits given to each eligible voter under `CreditMode.CONSTANT`.
    uint256 credits;
    /// @notice The timepoint that voting power is read at, in the ERC-6372 clock units of the
    /// token. Recorded when the round is requested, so it is the same for every input.
    uint48 snapshot;
    /// @notice Divides raw token voting power into the units the ballot is encoded in. Only used
    /// by `CensusMode.ONCHAIN`. Never zero for such a round.
    uint256 votingPowerDivisor;
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
  /// @notice Largest `decimals` a divisor can be derived from: `10 ** 77` is the last power of ten
  /// that fits in a uint256.
  uint8 constant MAX_DERIVABLE_DECIMALS = 78;
  // State variables
  IInterfold public interfold;
  IRiscZeroVerifier public risc0Verifier;
  bytes32 public imageId;
  /// @notice Verifies ballots for the census modes that prove membership of a Merkle tree.
  IHonkVerifier private immutable honkVerifier;
  /// @notice Verifies ballots for `CensusMode.ONCHAIN`, whose circuit has no Merkle inputs and
  /// takes voting power as a public input instead.
  IHonkVerifier private immutable onchainHonkVerifier;

  /// @notice The EIP-712 type of the message a voter signs to authorise one ballot.
  /// @dev The digest binds the signature to the round, the slot, and this exact ciphertext. The
  /// chain id and this contract address come from the EIP-712 domain, so a signature cannot be
  /// replayed onto another round, another slot, another ballot, or another deployment.
  bytes32 private constant BALLOT_TYPEHASH = keccak256("Ballot(uint256 e3Id,address slot,bytes32 ciphertextCommitment)");

  // Mappings
  mapping(uint256 e3Id => RoundData) e3Data;

  // Errors
  error CallerNotAuthorized();
  error E3AlreadyInitialized();
  error InterfoldAddressZero();
  error InterfoldAlreadyBound();
  error InterfoldNotContract();
  error ProgramNotRegistered();
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
  /// @notice `CensusMode.ONCHAIN` reads voting power from a token, so it cannot run without one
  /// that answers `getPastVotes`.
  error CensusModeRequiresToken();
  /// @notice An `ONCHAIN` round with constant credits must grant a non-zero allowance, or it
  /// bounds every ballot to zero and can only accept masks.
  error InvalidCredits();
  /// @notice The slot holds less voting power than the round requires, so it cannot be written to.
  /// @dev Raised for mask inputs as well as real votes. Under `CensusMode.ONCHAIN` this check
  /// replaces the Merkle membership proof that gates both branches in the other modes.
  error SlotNotEligible();
  error InvalidCensusMode();

  /// @notice A token reports more decimals than a divisor can be derived from.
  /// @dev `10 ** (decimals - 1)` must fit in a uint256. Pass an explicit divisor for such a token.
  error UnsupportedTokenDecimals(uint8 decimals);

  /// @notice An ONCHAIN round's floor is below one ballot unit.
  /// @dev `minVotingPower` must be at least the divisor, so every slot that clears the floor
  /// carries at least one unit of weight. Below that, a slot could be eligible to write yet scale
  /// to zero — able to publish, but unable to carry any weight, which is disenfranchisement that
  /// nothing on-chain would report.
  error MinVotingPowerBelowScale();
  error SlotIsEmpty();
  error MerkleRootNotSet();
  error InvalidNumOptions();
  error InputDeadlinePassed(uint256 e3Id, uint256 deadline);
  error KeyNotPublished(uint256 e3Id);
  error E3NotAcceptingInputs(uint256 e3Id);
  error InvalidComputeContext();

  // Events
  event InterfoldBound(address indexed interfold);
  event InputPublished(uint256 indexed e3Id, bytes encryptedVote, uint256 index);

  /// @notice Initialize the contract without an Interfold controller.
  /// @dev The owner binds the controller after Interfold registers this program.
  /// @param _initialOwner The account that can configure and bind this program.
  /// @param _risc0Verifier The RISC Zero verifier address
  /// @param _honkVerifier The honk verifier address
  /// @param _imageId The image ID for the guest program
  constructor(
    address _initialOwner,
    IRiscZeroVerifier _risc0Verifier,
    IHonkVerifier _honkVerifier,
    IHonkVerifier _onchainHonkVerifier,
    bytes32 _imageId
  ) Ownable(_initialOwner) EIP712("CRISP", "1") {
    if (address(_risc0Verifier) == address(0)) revert Risc0VerifierAddressZero();
    if (address(_honkVerifier) == address(0)) revert InvalidHonkVerifier();
    if (address(_onchainHonkVerifier) == address(0)) revert InvalidHonkVerifier();

    risc0Verifier = _risc0Verifier;
    honkVerifier = _honkVerifier;
    onchainHonkVerifier = _onchainHonkVerifier;
    imageId = _imageId;
  }

  /// @notice Bind this program to its permanent Interfold controller.
  /// @dev Interfold must register this program before the owner calls this function.
  /// @param _interfold The Interfold controller that registered this program.
  function bindInterfold(IInterfold _interfold) external onlyOwner {
    if (address(interfold) != address(0)) revert InterfoldAlreadyBound();
    if (address(_interfold) == address(0)) revert InterfoldAddressZero();
    if (address(_interfold).code.length == 0) revert InterfoldNotContract();
    if (!IInterfoldProgramRegistry(address(_interfold)).e3Programs(IE3Program(address(this)))) {
      revert ProgramNotRegistered();
    }

    interfold = _interfold;
    emit InterfoldBound(address(_interfold));
  }

  /// @notice The digest a voter signs to authorise one ballot.
  /// @dev Computed for every input, and for mask inputs as well as real votes. The circuit ignores
  /// it on the mask branch, but the contract must not skip it: a digest that were computed only
  /// for real votes would let an observer tell the two apart on-chain, which is exactly what mask
  /// inputs exist to prevent.
  /// @param e3Id The E3 the ballot belongs to.
  /// @param slot The slot address the ballot is written to.
  /// @param ciphertextCommitment The commitment to the ballot ciphertext.
  /// @return The EIP-712 digest.
  function ballotDigest(uint256 e3Id, address slot, bytes32 ciphertextCommitment) public view returns (bytes32) {
    return _hashTypedDataV4(keccak256(abi.encode(BALLOT_TYPEHASH, e3Id, slot, ciphertextCommitment)));
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

  /// @notice The divisor applied to raw token voting power for a `CensusMode.ONCHAIN` round.
  /// @dev A client must divide by exactly this before proving, because the contract passes the
  /// scaled value to the circuit as public input 4 and the proof is checked against it. Zero for
  /// rounds that are not ONCHAIN, where no scaling happens.
  /// @param e3Id The E3 to look up.
  /// @return The divisor recorded at validation.
  function votingPowerDivisorOf(uint256 e3Id) external view returns (uint256) {
    return e3Data[e3Id].votingPowerDivisor;
  }

  /// @notice The voting power a slot may spend in an ONCHAIN round, in ballot units.
  /// @dev The value `publishInput` will hand the circuit as public input 4, computed by the same
  /// contract that will check the proof. A client must prove against exactly this: recomputing it
  /// off-chain means re-deriving the snapshot, the divisor and the rounding, and any drift only
  /// shows up as an opaque verifier failure. Returns 0 for a round that is not ONCHAIN, where the
  /// bound comes from the census leaf instead.
  ///
  /// Does not apply the eligibility floor — it answers "how much weight", not "may this slot
  /// write". `publishInput` still enforces the floor.
  /// @param e3Id The round.
  /// @param slot The slot address.
  /// @return The spendable voting power, in ballot units.
  function votingPowerOf(uint256 e3Id, address slot) external view returns (uint256) {
    RoundData storage round = e3Data[e3Id];
    if (round.censusMode != CensusMode.ONCHAIN) return 0;
    if (round.creditMode == CreditMode.CONSTANT) return round.credits;

    return IVotesToken(round.token).getPastVotes(slot, round.snapshot) / round.votingPowerDivisor;
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

    // Delegated to its own frame rather than scoped inline: `validate` is close enough to the
    // stack limit that holding the six decoded values alongside the parameters exceeds it.
    _initRound(e3Id, customParams);

    e3Data[e3Id].paramsHash = keccak256(e3ProgramParams);

    // Initialize the votes Merkle tree for this E3 ID.
    e3Data[e3Id].votes._init(TREE_DEPTH);

    return ENCRYPTION_SCHEME_ID;
  }

  /// @notice Decode the round configuration and record it.
  /// @dev One decode, every field required. `censusMode` is read as a uint and range-checked
  /// rather than decoded straight into the enum, so an unrecognised value gives a named error
  /// instead of a bare panic.
  /// @param e3Id The E3 being configured.
  /// @param customParams The ABI-encoded round configuration.
  function _initRound(uint256 e3Id, bytes calldata customParams) internal {
    (
      address token,
      uint256 minVotingPower,
      uint256 numOptions,
      CreditMode creditMode,
      uint256 credits,
      uint256 rawCensusMode,
      uint256 votingPowerDivisor
    ) = abi.decode(customParams, (address, uint256, uint256, CreditMode, uint256, uint256, uint256));

    // The circuit asserts `num_options <= MAX_OPTIONS`, so a round configured above it accepts no
    // ballot at all. Reject at request time rather than stranding a round nobody can vote in.
    if (numOptions < 2 || numOptions > MAX_VOTE_OPTIONS) revert InvalidNumOptions();
    if (rawCensusMode > uint256(type(CensusMode).max)) revert InvalidCensusMode();

    // Rejected here rather than by the coordinator, so a combination that can never work costs
    // nothing: this reverts in the same transaction that requests the E3, before any fee is paid.
    if (CensusMode(rawCensusMode) == CensusMode.BY_REQUESTER && creditMode != CreditMode.CONSTANT) {
      revert CensusModeRequiresConstantCredits();
    }

    // ONCHAIN reads every voter's power from this token, so a round without one accepts no ballot
    // at all. Same reasoning as the numOptions bound: fail before the fee is paid.
    if (CensusMode(rawCensusMode) == CensusMode.ONCHAIN && token == address(0)) {
      revert CensusModeRequiresToken();
    }

    // An ONCHAIN round hands `credits` to the circuit as the voting-power bound, so zero credits
    // bound every ballot to zero: only a mask would be accepted, and the round would tally
    // nothing. Checked for ONCHAIN only — the Merkle modes take the bound from the census leaf,
    // where `credits` never reaches the circuit and the contract has nothing to check.
    if (CensusMode(rawCensusMode) == CensusMode.ONCHAIN && creditMode == CreditMode.CONSTANT && credits == 0) {
      revert InvalidCredits();
    }

    RoundData storage round = e3Data[e3Id];
    // we need to know the number of options for decoding the tally
    round.numOptions = numOptions;
    // we want to save the credit mode so it can be verified on chain by everyone
    round.creditMode = creditMode;
    // recorded so anyone can verify which electorate the round was requested against
    round.censusMode = CensusMode(rawCensusMode);
    round.token = token;
    round.minVotingPower = minVotingPower;
    round.credits = credits;

    // The snapshot is taken here rather than supplied by the requester. This function runs in the
    // transaction that requests the E3, so `clock() - 1` is the last finalized timepoint of the
    // round, and a requester cannot name a timepoint that suits it. Recording it once also makes
    // every input of the round read the same electorate.
    if (CensusMode(rawCensusMode) == CensusMode.ONCHAIN) {
      // Checked before any call is attempted. A call to an address with no code succeeds and
      // returns nothing, so `clock()` fails while decoding the empty return data rather than
      // inside the call — and a decode failure is not what `try/catch` is there to catch. An EOA
      // would otherwise be refused by a bare panic instead of a named error.
      if (token.code.length == 0) revert CensusModeRequiresToken();

      uint48 snapshot = _previousTimepoint(token);

      // Probe the exact call every input will make. `_previousTimepoint` swallows a missing
      // `clock()` and falls back to block numbers, which is right for a token that predates
      // ERC-6372 but also lets an address that is not a votes token pass validation — and then
      // every `publishInput` reverts inside `getPastVotes`, after the fee is paid.
      try IVotesToken(token).getPastVotes(address(0), snapshot) returns (uint256) {} catch {
        revert CensusModeRequiresToken();
      }

      round.snapshot = snapshot;

      // Derived only after the code check above. `decimals()` on a codeless address returns empty
      // data, and the failure happens while decoding rather than inside the call, which `try` does
      // not catch — deriving any earlier would refuse an EOA with a bare panic instead of the
      // named error the check above raises.
      uint256 divisor = votingPowerDivisor == 0 ? _defaultVotingPowerDivisor(token) : votingPowerDivisor;

      // Only CUSTOM credits take the circuit bound from scaled power; a CONSTANT round hands the
      // circuit `credits` and never reads the scaled value, so the floor and the divisor have
      // nothing to agree about there.
      //
      // Where they do meet, the floor is raw and the bound is scaled, so they only agree when the
      // floor is worth at least one ballot unit. Requiring it here means every slot that passes
      // `_eligibility` carries weight, and it costs nothing: this reverts in the transaction that
      // requests the E3, not per input after the fee is paid. It also keeps masks working — they
      // run the same eligibility check as real votes, so a slot that scaled to zero could not be
      // masked without revealing which inputs were masks.
      if (creditMode == CreditMode.CUSTOM && minVotingPower < divisor) revert MinVotingPowerBelowScale();

      round.votingPowerDivisor = divisor;
    }
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

    if (data.length == 0) revert EmptyInputData();

    (bytes memory noirProof, address slotAddress, bytes32 encryptedVoteCommitment, bytes memory encryptedVote) = abi.decode(
      data,
      (bytes, address, bytes32, bytes)
    );

    // The two census families differ here and nowhere else. A Merkle round proves membership
    // inside the circuit against a posted root. An ONCHAIN round reads the power from the token
    // and gives it to the circuit, so the eligibility check has to happen here instead.
    (bytes32 eligibility, IHonkVerifier verifier) = _eligibility(e3Id, slotAddress);

    (uint40 voteIndex, bytes32 previousEncryptedVoteCommitment) = _processVote(e3Id, slotAddress, encryptedVoteCommitment);

    // Set the public inputs for the proof. Order must match Noir circuit.
    bytes32[] memory noirPublicInputs = new bytes32[](9);
    noirPublicInputs[0] = previousEncryptedVoteCommitment;
    // A Keccak digest does not fit in one field element, so it enters the circuit as its two
    // 16-byte halves. The circuit rebuilds the 32 bytes with `digest_from_halves`.
    {
      uint256 digest = uint256(ballotDigest(e3Id, slotAddress, encryptedVoteCommitment));
      noirPublicInputs[1] = bytes32(digest >> 128);
      noirPublicInputs[2] = bytes32(digest & type(uint128).max);
    }
    noirPublicInputs[3] = bytes32(uint256(uint160(slotAddress)));
    noirPublicInputs[4] = eligibility;
    noirPublicInputs[5] = bytes32(uint256(previousEncryptedVoteCommitment == bytes32(0) ? 1 : 0));
    noirPublicInputs[6] = bytes32(e3Data[e3Id].numOptions);
    noirPublicInputs[7] = encryptedVoteCommitment;
    noirPublicInputs[8] = e3.committeePublicKey;

    // Check if the ciphertext was encrypted correctly
    if (!verifier.verify(noirProof, noirPublicInputs)) {
      revert InvalidNoirProof();
    }

    emit InputPublished(e3Id, encryptedVote, voteIndex);
  }

  /// @notice Resolve the eligibility public input and the verifier for a round.
  /// @dev Returns the value that occupies index 4 of the circuit public inputs. The `crisp` and
  /// `crisp_onchain` circuits agree on every other position, so this one value and the verifier
  /// address are the whole difference between the two paths.
  /// @param e3Id The E3 the input belongs to.
  /// @param slotAddress The slot the input is written to.
  /// @return eligibility The Merkle root of the census, or the voting power of the slot.
  /// @return verifier The verifier that matches the circuit of this round.
  function _eligibility(uint256 e3Id, address slotAddress) internal view returns (bytes32 eligibility, IHonkVerifier verifier) {
    RoundData storage round = e3Data[e3Id];

    if (round.censusMode != CensusMode.ONCHAIN) {
      // We need to ensure that the CRISP admin set the merkle root of the census.
      if (round.merkleRoot == 0) revert MerkleRootNotSet();
      return (bytes32(round.merkleRoot), honkVerifier);
    }

    uint256 rawPower = IVotesToken(round.token).getPastVotes(slotAddress, round.snapshot);

    // The floor is compared against RAW power, in the token's own units. `minVotingPower` is a
    // governance setting ("you need N tokens to vote") written the way every other token plugin
    // writes it, so scaling it here would reinterpret a configured value by the divisor.
    uint256 threshold = round.minVotingPower == 0 ? 1 : round.minVotingPower;
    if (rawPower < threshold) revert SlotNotEligible();

    // Scaled only for the circuit. It enforces `vote <= voting_power`, and the BFV encoding caps
    // each choice at `2**(100/numOptions) - 1` — about 8.6e9 for three options. Raw power from an
    // 18-decimal token is ~1e18 per token, so handing it over unscaled would put every holder
    // above the cap and collapse token weighting into a flat ceiling. Dividing mirrors what the
    // coordinator does when it builds a Merkle census (`balance / 10**(decimals - 1)`), so both
    // census families encode ballots in the same units and a tally decodes the same way.
    uint256 power = rawPower / round.votingPowerDivisor;


    // Eligibility comes from the power at the snapshot. The weight the circuit enforces comes from
    // the credit mode, so a CONSTANT round gives every eligible slot the same credits.
    return (bytes32(round.creditMode == CreditMode.CONSTANT ? round.credits : power), onchainHonkVerifier);
  }

  /// @notice The divisor to apply to raw voting power when the requester does not name one.
  /// @dev Mirrors the coordinator's census scaling (`balance / 10**(decimals - 1)`), so an ONCHAIN
  /// round and a Merkle round over the same token encode ballots in identical units. `decimals()`
  /// is optional on an ERC20, so a token without it is left unscaled rather than rejected — a
  /// requester that needs scaling for such a token passes an explicit divisor.
  /// @param token The token voting power is read from.
  /// @return The divisor, never zero.
  function _defaultVotingPowerDivisor(address token) internal view returns (uint256) {
    try IVotesToken(token).decimals() returns (uint8 dec) {
      // `10 ** 78` does not fit in a uint256, and the exponentiation happens in the success body
      // of the `try`, where a revert is NOT caught — an absurd `decimals` would surface as a bare
      // arithmetic panic instead of a named error, which is the failure mode the code check above
      // exists to avoid. Refused explicitly; such a token can still be used by naming a divisor.
      if (dec > MAX_DERIVABLE_DECIMALS) revert UnsupportedTokenDecimals(dec);

      return dec > 1 ? 10 ** (uint256(dec) - 1) : 1;
    } catch {
      return 1;
    }
  }

  /// @notice The last finalized timepoint of a token, in its ERC-6372 clock units.
  /// @dev Falls back to block numbers when the token has no `clock()`, which matches the default
  /// ERC20Votes clock.
  /// @param token The token to read the clock of.
  /// @return The timepoint before the current one.
  function _previousTimepoint(address token) internal view returns (uint48) {
    try IERC6372Clock(token).clock() returns (uint48 current) {
      return current == 0 ? 0 : current - 1;
    } catch {
      return uint48(block.number - 1);
    }
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
