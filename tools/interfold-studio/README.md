# 🔒 The Interfold E3 Studio & BFV Decoder Suite

An interactive **Encrypted Execution Environment (E3) Lifecycle Simulator**, **BFV FHE Parameter Preset Inspector**, and **u64 Plaintext Hex Decoder** for **The Interfold (`theinterfold/interfold`)**.

---

## 🌟 Key Features

- 🔒 **Encrypted Execution Environments (E3)**: Activate confidential multi-party computation instances with threshold BFV encryption.
- 🔑 **u64 Precision-Safe Plaintext Decoder**: Decode little-endian plaintext outputs without BigInt precision loss.
- 🌐 **Interactive Web Studio**: Real-time E3 inspector and decoder console on `http://localhost:3431`.
- ⌨️ **Universal CLI (`interfold-cli`)**: Terminal utility for activating E3s and decoding outputs.

---

## 🚀 Quickstart

```bash
# Launch Interfold Studio
npm start
# Open http://localhost:3431

# Or run via CLI
node bin/interfold-cli.js request
node bin/interfold-cli.js decode 0x2a00000000000000
```
