/**
 * Interfold E3 Unit Tests
 */

import { defaultInterfoldE3Engine } from '../src/core/e3-engine.js';

async function runE3Tests() {
  console.log('Testing The Interfold E3 Engine & BFV Ciphertext Decoder...');

  // 1. Request E3 Instance
  const e3 = defaultInterfoldE3Engine.requestE3({
    programId: 'e3_confidential_auction',
    presetName: 'INSECURE_THRESHOLD_512',
  });
  if (!e3.e3Id || e3.stage !== 'ACTIVATED') {
    throw new Error('E3 request failed');
  }

  // 2. Encrypt Input
  const result = defaultInterfoldE3Engine.submitEncryptedInput({ e3Id: e3.e3Id, valueNumber: 42 });
  if (!result.ciphertext || !result.proof) {
    throw new Error('E3 BFV input encryption failed');
  }

  // 3. Decode Plaintext Hex Output
  const decoded = defaultInterfoldE3Engine.decodePlaintextHex('0x2a00000000000000');
  if (!decoded || decoded.bigIntString !== '42' || decoded.numberValue !== 42) {
    throw new Error('BFV Plaintext output hex decoding failed');
  }

  console.log(`✅ Interfold E3 Instance #${e3.e3Id} & BFV Plaintext Decoder Verified!`);
}

runE3Tests().catch(e => {
  console.error('❌ E3 Test Failed:', e);
  process.exit(1);
});
