// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
pragma solidity >=0.8.27;

/// @notice The subset of a generated Honk verifier that `CRISPProgram` calls.
/// @dev Declared as an interface so the census paths can hold verifiers generated from different
/// circuits. Every generated verifier declares a contract named `HonkVerifier`, and there is one
/// per census mode per BFV preset under `contracts/verifiers/<preset>/`, so importing the concrete
/// types would collide. Deployment resolves the right one by fully qualified name; see
/// `scripts/verifiers.ts`.
interface IHonkVerifier {
  /// @notice Verify a folded ballot proof against its public inputs.
  /// @dev Reverts rather than returning false when the public inputs do not match the proof.
  /// @param proof The folded proof.
  /// @param publicInputs The public inputs, in the order the fold circuit declares them.
  /// @return True when the proof is valid.
  function verify(bytes calldata proof, bytes32[] calldata publicInputs) external view returns (bool);
}
