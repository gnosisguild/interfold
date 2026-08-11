#!/usr/bin/env node

/**
 * The Interfold E3 CLI
 */

import { defaultInterfoldE3Engine } from '../src/core/e3-engine.js';

const args = process.argv.slice(2);
const command = args[0] || 'help';

async function main() {
  switch (command.toLowerCase()) {
    case 'request': {
      console.log('\n🔒 Requesting new Encrypted Execution Environment (E3)...');
      const e3 = defaultInterfoldE3Engine.requestE3({
        programId: 'e3_confidential_auction',
        presetName: 'INSECURE_THRESHOLD_512',
      });
      console.log(`  E3 ID:         #${e3.e3Id}`);
      console.log(`  Stage:         ${e3.stage}`);
      console.log(`  PK Commitment: ${e3.pkCommitment}`);
      console.log(`  Public Key:    ${e3.publicKey.slice(0, 24)}...\n`);
      break;
    }

    case 'decode': {
      const hex = args[1] || '0x2a00000000000000';
      console.log(`\n🔑 Decoding Plaintext Output Hex: "${hex}"...`);
      const d = defaultInterfoldE3Engine.decodePlaintextHex(hex);
      if (d) {
        console.log(`  BigInt Value:    ${d.bigIntString}`);
        console.log(`  Number Value:    ${d.numberValue}`);
        console.log(`  Precision Safe:  ${d.isPrecisionSafe}\n`);
      } else {
        console.log('  ❌ Invalid or short plaintext output hex\n');
      }
      break;
    }

    case 'studio': {
      console.log('\n🌐 Launching Interfold Studio on :3431...');
      await import('../src/server/app.js');
      break;
    }

    default: {
      console.log(`
╔══════════════════════════════════════════════════════════════════╗
║               🔒 THE INTERFOLD E3 PROTOCOL CLI                   ║
║  Encrypted Execution Environment, BFV FHE & Decoder Suite        ║
╚══════════════════════════════════════════════════════════════════╝

Commands:
  interfold-cli request                 Request a new E3 instance
  interfold-cli decode [hex]            Decode u64 plaintext output hex string
  interfold-cli studio                  Launch Interactive Web Studio on :3431
      `);
      break;
    }
  }
}

main().catch(err => {
  console.error('Error:', err.message);
  process.exit(1);
});
