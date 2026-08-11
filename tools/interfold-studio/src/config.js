/**
 * Interfold Encrypted Execution Environment (E3) Configuration
 */

export const INTERFOLD_CONFIG = {
  protocol: {
    name: 'The Interfold Protocol (Encrypted Execution Environments)',
    formerName: 'Enclave Protocol (Gnosis Guild)',
    encryptionScheme: 'BFV (Brakerski-Fan-Vercauteren) Homomorphic Encryption',
    zkProver: 'Noir ZK Circuits & RISC Zero ZKVM',
    mpcEngine: 'Threshold Key Generation & Decryption Committee',
  },
  bfvPresets: [
    {
      name: 'INSECURE_THRESHOLD_512',
      degree: 512,
      plaintextModulus: 65537,
      securityLevel: 'Fast Local Development / Testing',
      ciphertextByteSize: 9242,
    },
    {
      name: 'SAFE_THRESHOLD_2048',
      degree: 2048,
      plaintextModulus: 65537,
      securityLevel: '128-bit Production Cryptographic Security',
      ciphertextByteSize: 36968,
    },
  ],
  sampleE3Programs: [
    {
      id: 'e3_confidential_auction',
      title: 'Confidential Blind Auction E3',
      description: 'Executes sealed-bid auctions with encrypted bids; outputs highest bidder without revealing losing amounts.',
      inputWindowDuration: 1800,
    },
    {
      id: 'e3_private_sortition',
      title: 'Private Ciphernode Sortition E3',
      description: 'Selects verifiable committee members via threshold randomness without revealing validator balances.',
      inputWindowDuration: 3600,
    },
  ],
};
