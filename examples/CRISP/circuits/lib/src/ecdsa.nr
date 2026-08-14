// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use keccak256::keccak256;

/// Validates an ECDSA signature over a hashed message.
///
/// This function validates that a signature was created by the holder of the private key
/// corresponding to the provided public key, and that it signs the given hashed message.
///
/// # Arguments
/// * `hashed_message` - The Keccak256 hash of the message being signed (32 bytes)
/// * `pub_key_x` - The x-coordinate of the public key (32 bytes, big-endian)
/// * `pub_key_y` - The y-coordinate of the public key (32 bytes, big-endian)
/// * `signature` - The ECDSA signature (64 bytes: r || s, both 32 bytes big-endian)
///
/// # Returns
/// Returns `true` if the signature is valid, `false` otherwise.
///
/// # Note
/// The signature is verified using secp256k1 curve (same as Ethereum). The message
/// should already be hashed before calling this function.
pub fn validate_signature(
    hashed_message: [u8; 32],
    pub_key_x: [u8; 32],
    pub_key_y: [u8; 32],
    signature: [u8; 64],
) -> bool {
    std::ecdsa_secp256k1::verify_signature(pub_key_x, pub_key_y, signature, hashed_message)
}

/// Derives an Ethereum address from a secp256k1 public key.
///
/// This function implements the standard Ethereum address derivation algorithm:
/// 1. Concatenate the public key coordinates (64 bytes total)
/// 2. Compute Keccak256 hash of the concatenated bytes
/// 3. Extract the last 20 bytes as the address
///
/// # Arguments
/// * `pub_key_x` - The x-coordinate of the public key (32 bytes, big-endian)
/// * `pub_key_y` - The y-coordinate of the public key (32 bytes, big-endian)
///
/// # Returns
/// A 20-byte array representing the Ethereum address.
pub fn derive_address(pub_key_x: [u8; 32], pub_key_y: [u8; 32]) -> [u8; 20] {
    let mut pub_key_bytes = [0; 64];
    for i in 0..32 {
        pub_key_bytes[i] = pub_key_x[i];
        pub_key_bytes[i + 32] = pub_key_y[i];
    }

    // Hash the concatenated public key
    let hash = keccak256(pub_key_bytes, 64);

    // Extract last 20 bytes as the Ethereum address
    let mut derived_address = [0; 20];
    for i in 0..20 {
        derived_address[i] = hash[i + 12];
    }

    derived_address
}

/// Converts a 20-byte Ethereum address to a field element.
///
/// This function interprets the address bytes as a big-endian integer and converts it
/// to a field element.
///
/// # Arguments
/// * `addr` - A 20-byte Ethereum address
///
/// # Returns
/// A field element representing the address as an integer.
pub fn address_to_field(addr: [u8; 20]) -> Field {
    let mut acc: Field = 0;

    for i in 0..20 {
        acc = acc * 256 + (addr[i] as Field);
    }

    acc
}

/// Rebuilds the 32-byte signed digest from its two field halves.
///
/// A Keccak-256 digest is 256 bits, but a field element holds fewer than 254 bits. The digest
/// therefore enters a circuit as two public inputs. `digest_hi` holds the first 16 bytes and
/// `digest_lo` holds the last 16 bytes. Both halves are big-endian.
///
/// `to_be_bytes` constrains each half to 16 bytes, so each half has one representation only.
///
/// # Arguments
/// * `digest_hi` - The first 16 bytes of the digest, as a big-endian integer
/// * `digest_lo` - The last 16 bytes of the digest, as a big-endian integer
///
/// # Returns
/// The 32-byte digest.
pub fn digest_from_halves(digest_hi: Field, digest_lo: Field) -> [u8; 32] {
    let hi: [u8; 16] = digest_hi.to_be_bytes();
    let lo: [u8; 16] = digest_lo.to_be_bytes();

    let mut digest = [0; 32];
    for i in 0..16 {
        digest[i] = hi[i];
        digest[i + 16] = lo[i];
    }

    digest
}

#[test]
fn test_derive_address() {
    let pub_key_x = [
        131, 24, 83, 91, 84, 16, 93, 74, 122, 174, 96, 192, 143, 196, 95, 150, 135, 24, 27, 79, 223,
        198, 37, 189, 26, 117, 63, 167, 57, 127, 237, 117,
    ];
    let pub_key_y = [
        53, 71, 241, 28, 168, 105, 102, 70, 242, 243, 172, 176, 142, 49, 1, 106, 250, 194, 62, 99,
        12, 93, 17, 245, 159, 97, 254, 245, 123, 13, 42, 165,
    ];
    let address = derive_address(pub_key_x, pub_key_y);
    assert(
        address
            == [
                243, 159, 214, 229, 26, 173, 136, 246, 244, 206, 106, 184, 130, 114, 121, 207, 255,
                185, 34, 102,
            ],
    );
}

#[test]
fn test_validate_signature() {
    let hashed_message = [
        67, 126, 157, 164, 162, 165, 56, 242, 155, 214, 113, 196, 83, 198, 228, 36, 174, 104, 152,
        87, 167, 108, 64, 34, 234, 161, 122, 55, 44, 62, 151, 55,
    ];
    let pub_key_x = [
        131, 24, 83, 91, 84, 16, 93, 74, 122, 174, 96, 192, 143, 196, 95, 150, 135, 24, 27, 79, 223,
        198, 37, 189, 26, 117, 63, 167, 57, 127, 237, 117,
    ];
    let pub_key_y = [
        53, 71, 241, 28, 168, 105, 102, 70, 242, 243, 172, 176, 142, 49, 1, 106, 250, 194, 62, 99,
        12, 93, 17, 245, 159, 97, 254, 245, 123, 13, 42, 165,
    ];
    let signature = [
        78, 219, 63, 96, 214, 92, 16, 36, 108, 207, 133, 108, 3, 248, 122, 23, 192, 236, 210, 253,
        206, 125, 83, 124, 163, 54, 106, 125, 188, 223, 192, 39, 44, 76, 228, 107, 188, 221, 183,
        124, 118, 143, 228, 126, 216, 173, 160, 231, 62, 52, 188, 154, 110, 230, 183, 71, 36, 161,
        171, 163, 213, 62, 223, 152,
    ];
    assert(validate_signature(hashed_message, pub_key_x, pub_key_y, signature));
}

#[test]
fn test_validate_signature_sdk_input() {
    let hashed_message = [
        200, 232, 98, 162, 80, 131, 242, 57, 252, 76, 226, 45, 127, 206, 207, 39, 206, 44, 211, 171,
        113, 67, 121, 68, 78, 253, 202, 79, 29, 128, 130, 76,
    ];

    let pub_key_x = [
        131, 24, 83, 91, 84, 16, 93, 74, 122, 174, 96, 192, 143, 196, 95, 150, 135, 24, 27, 79, 223,
        198, 37, 189, 26, 117, 63, 167, 57, 127, 237, 117,
    ];
    let pub_key_y = [
        53, 71, 241, 28, 168, 105, 102, 70, 242, 243, 172, 176, 142, 49, 1, 106, 250, 194, 62, 99,
        12, 93, 17, 245, 159, 97, 254, 245, 123, 13, 42, 165,
    ];
    let signature = [
        22, 65, 67, 29, 14, 211, 253, 134, 129, 79, 2, 109, 166, 46, 17, 67, 75, 83, 198, 168, 81,
        98, 254, 167, 249, 146, 24, 191, 60, 48, 125, 236, 127, 54, 28, 35, 95, 7, 182, 88, 120, 10,
        253, 145, 165, 201, 214, 141, 106, 75, 20, 213, 235, 5, 17, 246, 104, 141, 62, 145, 20, 14,
        236, 18,
    ];

    assert(validate_signature(hashed_message, pub_key_x, pub_key_y, signature));
}

#[test]
fn test_digest_from_halves() {
    let digest = digest_from_halves(
        0x437e9da4a2a538f29bd671c453c6e424,
        0xae689857a76c4022eaa17a372c3e9737,
    );

    assert(
        digest
            == [
                67, 126, 157, 164, 162, 165, 56, 242, 155, 214, 113, 196, 83, 198, 228, 36, 174, 104,
                152, 87, 167, 108, 64, 34, 234, 161, 122, 55, 44, 62, 151, 55,
            ],
    );
}

#[test]
fn test_digest_from_halves_matches_signature() {
    // The same digest as `test_validate_signature`, supplied as two halves.
    let digest = digest_from_halves(
        0x437e9da4a2a538f29bd671c453c6e424,
        0xae689857a76c4022eaa17a372c3e9737,
    );
    let pub_key_x = [
        131, 24, 83, 91, 84, 16, 93, 74, 122, 174, 96, 192, 143, 196, 95, 150, 135, 24, 27, 79, 223,
        198, 37, 189, 26, 117, 63, 167, 57, 127, 237, 117,
    ];
    let pub_key_y = [
        53, 71, 241, 28, 168, 105, 102, 70, 242, 243, 172, 176, 142, 49, 1, 106, 250, 194, 62, 99,
        12, 93, 17, 245, 159, 97, 254, 245, 123, 13, 42, 165,
    ];
    let signature = [
        78, 219, 63, 96, 214, 92, 16, 36, 108, 207, 133, 108, 3, 248, 122, 23, 192, 236, 210, 253,
        206, 125, 83, 124, 163, 54, 106, 125, 188, 223, 192, 39, 44, 76, 228, 107, 188, 221, 183,
        124, 118, 143, 228, 126, 216, 173, 160, 231, 62, 52, 188, 154, 110, 230, 183, 71, 36, 161,
        171, 163, 213, 62, 223, 152,
    ];

    assert(validate_signature(digest, pub_key_x, pub_key_y, signature));
}

/// A signature is valid only for the digest it covers.
///
/// This is the property the ballot binding depends on. The digest is a public input that the
/// contract computes from the round, the slot, and the ballot, so a signature taken from anywhere
/// else cannot be replayed onto a different ballot. Before the digest became a public input, the
/// message was a free witness and any signature by the slot key was accepted.
#[test(should_fail)]
fn test_fail_signature_over_different_digest() {
    // The key and signature of `test_validate_signature`, checked against the digest of
    // `test_validate_signature_sdk_input`.
    let other_digest = digest_from_halves(
        0xc8e862a25083f239fc4ce22d7fcecf27,
        0xce2cd3ab714379444efdca4f1d80824c,
    );
    let pub_key_x = [
        131, 24, 83, 91, 84, 16, 93, 74, 122, 174, 96, 192, 143, 196, 95, 150, 135, 24, 27, 79, 223,
        198, 37, 189, 26, 117, 63, 167, 57, 127, 237, 117,
    ];
    let pub_key_y = [
        53, 71, 241, 28, 168, 105, 102, 70, 242, 243, 172, 176, 142, 49, 1, 106, 250, 194, 62, 99,
        12, 93, 17, 245, 159, 97, 254, 245, 123, 13, 42, 165,
    ];
    let signature = [
        78, 219, 63, 96, 214, 92, 16, 36, 108, 207, 133, 108, 3, 248, 122, 23, 192, 236, 210, 253,
        206, 125, 83, 124, 163, 54, 106, 125, 188, 223, 192, 39, 44, 76, 228, 107, 188, 221, 183,
        124, 118, 143, 228, 126, 216, 173, 160, 231, 62, 52, 188, 154, 110, 230, 183, 71, 36, 161,
        171, 163, 213, 62, 223, 152,
    ];

    assert(validate_signature(other_digest, pub_key_x, pub_key_y, signature));
}

#[test(should_fail)]
fn test_fail_digest_half_too_large() {
    // A half that needs more than 16 bytes has no unique encoding, so `to_be_bytes` must reject it.
    let _ = digest_from_halves(0x0100000000000000000000000000000000, 0);
}

#[test(should_fail)]
fn test_fail_validate_signature() {
    // This test verifies that invalid signatures cause validation to fail.
    // The test itself will fail (assertion failure) when validate_signature is called
    // with an invalid signature, which demonstrates that invalid signatures are properly rejected.
    let hashed_message = [
        67, 126, 157, 164, 162, 165, 56, 242, 155, 214, 113, 196, 83, 198, 228, 36, 174, 104, 152,
        87, 167, 108, 64, 34, 234, 161, 122, 55, 44, 62, 151, 55,
    ];
    let pub_key_x = [
        131, 24, 83, 91, 84, 16, 93, 74, 122, 174, 96, 192, 143, 196, 95, 150, 135, 24, 27, 79, 223,
        198, 37, 189, 26, 117, 63, 167, 57, 127, 237, 117,
    ];
    let pub_key_y = [
        53, 71, 241, 28, 168, 105, 102, 70, 242, 243, 172, 176, 142, 49, 1, 106, 250, 194, 62, 99,
        12, 93, 17, 245, 159, 97, 254, 245, 123, 13, 42, 165,
    ];
    let signature = [
        78, 219, 63, 96, 214, 92, 16, 36, 108, 207, 133, 108, 3, 248, 122, 23, 192, 236, 210, 253,
        206, 125, 83, 124, 163, 54, 106, 125, 188, 223, 192, 39, 44, 76, 228, 107, 188, 221, 183,
        124, 118, 143, 228, 126, 216, 173, 160, 231, 62, 52, 188, 154, 110, 230, 183, 71, 36, 161,
        171, 163, 213, 62, 223, 151,
    ];

    // This call should fail (assertion failure) because the signature is invalid
    assert(validate_signature(hashed_message, pub_key_x, pub_key_y, signature));
}

#[test]
fn test_address_to_field() {
    let address: [u8; 20] = [
        243, 159, 214, 229, 26, 173, 136, 246, 244, 206, 106, 184, 130, 114, 121, 207, 255, 185, 34,
        102,
    ];
    let field = address_to_field(address);

    assert(field == 0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266);
}
