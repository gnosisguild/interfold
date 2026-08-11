/**
 * Interfold E3 Studio Web Server
 */

import express from 'express';
import cors from 'cors';
import path from 'path';
import { fileURLToPath } from 'url';
import { INTERFOLD_CONFIG } from '../config.js';
import { defaultInterfoldE3Engine } from '../core/e3-engine.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WEB_ROOT = path.join(__dirname, '../../web');

const app = express();
const PORT = process.env.PORT || 3431;

app.use(cors());
app.use(express.json());
app.use(express.static(WEB_ROOT));

// 1. Config & Presets
app.get('/api/config', (req, res) => {
  res.json({
    protocol: INTERFOLD_CONFIG.protocol,
    presets: INTERFOLD_CONFIG.bfvPresets,
    programs: INTERFOLD_CONFIG.sampleE3Programs,
  });
});

// 2. Request E3
app.post('/api/e3/request', (req, res) => {
  try {
    const e3 = defaultInterfoldE3Engine.requestE3(req.body);
    res.json({ success: true, e3 });
  } catch (err) {
    res.status(400).json({ error: err.message });
  }
});

// 3. Submit Encrypted Input
app.post('/api/e3/encrypt', (req, res) => {
  try {
    const result = defaultInterfoldE3Engine.submitEncryptedInput(req.body);
    res.json({ success: true, result });
  } catch (err) {
    res.status(400).json({ error: err.message });
  }
});

// 4. Decode Plaintext Hex Output
app.post('/api/e3/decode', (req, res) => {
  const { hex } = req.body;
  const decoded = defaultInterfoldE3Engine.decodePlaintextHex(hex || '0x2a00000000000000');
  res.json({ decoded });
});

if (process.env.NODE_ENV !== 'test') {
  app.listen(PORT, () => {
    console.log(`\n======================================================`);
    console.log(`🔒 The Interfold Encrypted Execution Environment (E3) Studio Running!`);
    console.log(`🌐 Web Dashboard: http://localhost:${PORT}`);
    console.log(`🛡️  Crypto: BFV FHE + Threshold MPC + ZK Proof Verification`);
    console.log(`======================================================\n`);
  });
}

export default app;
