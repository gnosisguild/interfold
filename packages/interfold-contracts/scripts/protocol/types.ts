// SPDX-License-Identifier: LGPL-3.0-only

export interface TimeoutConfig {
  dkgWindow: string;
  computeWindow: string;
  decryptionWindow: string;
}

export interface PricingConfig {
  keyGenFixedPerNode: string;
  keyGenPerEncryptionProof: string;
  coordinationPerPair: string;
  availabilityPerNodePerSec: string;
  decryptionPerNode: string;
  publicationBase: string;
  verificationPerProof: string;
  protocolTreasury: string;
  marginBps: string;
  protocolShareBps: string;
  dkgUtilizationBps: string;
  computeUtilizationBps: string;
  decryptUtilizationBps: string;
  minCommitteeSize: string;
  minThreshold: string;
}

export interface ProtocolConfigFile {
  name: string;
  chainId: number;
  /** Address that owns the protocol contracts and executes the wiring calls. */
  protocolOwner: string;
  /** Optional Safe address when the protocol owner is itself a Safe. */
  safe?: string;
  /** Optional Aragon Admin plugin route for DAO-owned protocol deployments. */
  governance?: {
    adminPlugin: string;
    proposerSafe: string;
    proposalMetadata?: string;
  };
  fold: string;
  /**
   * Optional escrow IVotes adapter. When set, only FOLD locked in that escrow carries voting
   * power and idle wallet FOLD carries none — operators keep their weight by bonding instead.
   * Omit to count wallet-held FOLD, which is the original behaviour.
   */
  escrowVotesAdapter?: string;
  bondingRegistryProxy: string;
  bondingRegistryProxyAdmin: string;
  feeToken: string;
  feeTokenDecimals: number;
  ticketUnderlyingToken: string;
  protocolTreasury: string;
  slashedFundsTreasury: string;
  slasher: string;
  ticketToken: { lockRegistry: boolean };
  bonding: {
    ticketPrice: string;
    requiredCiphernodeBond: string;
    ticketTokenDecimals: number;
    ciphernodeBondTokenDecimals: number;
    minTicketBalance: string;
    exitDelay: string;
  };
  /**
   * Bond owners to resynchronize when a `bondingRegistry` upgrade first attaches
   * `BondedCheckpoints`. Configuring the contract does not backfill, so an owner that bonded
   * beforehand reads as zero bonded voting power until its next bond, slash, exit claim or owner
   * transfer. Collect the list off-chain from `BondOwnerSet` and `CiphernodeBondUpdated` logs. Only
   * needed on a deployment that already has bonds; a first-time deployment has none.
   */
  bondedResyncOwners?: string[];
  registry: { sortitionSubmissionWindow: string };
  slashing: { initialDelay: string };
  interfold: {
    maxDuration: string;
    markFailedGracePeriod: string;
    timeoutConfig: TimeoutConfig;
    pricing: PricingConfig;
    committeeThresholds: Array<{
      size: string;
      quorum: string;
      total: string;
    }>;
    registerActiveBfvParamSet: boolean;
    allowFeeToken: boolean;
  };
  verifiers?: {
    /** Deploy the generated BFV verifier stack with this protocol deployment. */
    deploy?: boolean;
    decryptionVerifier?: string;
    pkVerifier?: string;
    dkgFoldAttestationVerifier?: string;
  };
  ciphertextVerifier?: string;
  /** Deploy the stateless MockE3Program as the initial E3 program. */
  deployMockE3Program?: boolean;
  bindInitialE3Program?: boolean;
  e3Programs: [string];
}

export interface ProtocolDeployment {
  name: string;
  chainId: number;
  operator: string;
  protocolOwner: string;
  safe?: string;
  fold: string;
  feeToken: string;
  ticketUnderlyingToken: string;
  bondingRegistryProxy: string;
  bondingRegistryProxyAdmin: string;
  bondingRegistryImplementation: string;
  bondingAssetLib: string;
  bondingEligibilityLib: string;
  bondingSlashingLib: string;
  bondingRegistrationLib: string;
  bondingOwnershipLib: string;
  bondedCheckpoints: string;
  /**
   * Deployed by `--action activate-voting`, after the governance batch initializes the registry: the
   * `BondedVotes` constructor rejects a registry that does not yet bond the token it reads.
   */
  bondedVotes?: string;
  decryptionVerifier?: string;
  pkVerifier?: string;
  dkgFoldAttestationVerifier?: string;
  dkgAggregatorVerifier?: string;
  decryptionAggregatorVerifier?: string;
  verifierZkTranscriptLib?: string;
  dkgVerifierRelationsLib?: string;
  decryptionVerifierRelationsLib?: string;
  initialE3Program: string;
  ticketToken: string;
  slashingManager: string;
  slashingEvidenceLib: string;
  poseidonT3: string;
  registrySortitionLib: string;
  ciphernodeRegistry: string;
  ciphernodeRegistryImplementation: string;
  ciphernodeRegistryProxyAdmin: string;
  interfold: string;
  interfoldImplementation: string;
  interfoldProxyAdmin: string;
  interfoldLifecycle: string;
  interfoldPricing: string;
  e3RefundManager: string;
  e3RefundManagerImplementation: string;
  e3RefundManagerProxyAdmin: string;
  safeTransactions: string;
  governanceSafeBuilder?: string;
  safeProposal?: SafeProposal;
}

export interface SafeTransaction {
  to: string;
  value: string;
  data: string;
  operation: number;
  contractMethod: null;
  contractInputsValues: null;
}

export interface SafeProposal {
  safeTxHash: string;
  safeAddress: string;
  proposer: string;
  nonce: number;
  transactionCount: number;
  origin: string;
  url?: string;
  proposedAt: string;
}

export interface ProtocolContracts {
  ticketToken: string;
  slashingManager: string;
  slashingEvidenceLib: string;
  poseidonT3: string;
  registrySortitionLib: string;
  ciphernodeRegistry: string;
  ciphernodeRegistryImplementation: string;
  ciphernodeRegistryProxyAdmin: string;
  interfold: string;
  interfoldImplementation: string;
  interfoldProxyAdmin: string;
  interfoldLifecycle: string;
  interfoldPricing: string;
  e3RefundManager: string;
  e3RefundManagerImplementation: string;
  e3RefundManagerProxyAdmin: string;
  bondingRegistryImplementation: string;
  bondingAssetLib: string;
  bondingEligibilityLib: string;
  bondingSlashingLib: string;
  bondingRegistrationLib: string;
  bondingOwnershipLib: string;
  bondedCheckpoints: string;
  /**
   * Deployed by `--action activate-voting`, after the governance batch initializes the registry: the
   * `BondedVotes` constructor rejects a registry that does not yet bond the token it reads.
   */
  bondedVotes?: string;
  decryptionVerifier?: string;
  pkVerifier?: string;
  dkgFoldAttestationVerifier?: string;
  dkgAggregatorVerifier?: string;
  decryptionAggregatorVerifier?: string;
  verifierZkTranscriptLib?: string;
  dkgVerifierRelationsLib?: string;
  decryptionVerifierRelationsLib?: string;
  initialE3Program: string;
}

export interface ProtocolInterfaces {
  ticket: {
    encodeFunctionData: (name: string, values?: readonly unknown[]) => string;
  };
  slashing: {
    encodeFunctionData: (name: string, values?: readonly unknown[]) => string;
  };
  registry: {
    encodeFunctionData: (name: string, values?: readonly unknown[]) => string;
  };
  interfold: {
    encodeFunctionData: (name: string, values?: readonly unknown[]) => string;
  };
  bonding: {
    encodeFunctionData: (name: string, values?: readonly unknown[]) => string;
  };
}

export interface ProtocolDeployResult {
  contracts: ProtocolContracts;
  interfaces: ProtocolInterfaces;
}
