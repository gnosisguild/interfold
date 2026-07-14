// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use bincode::{DefaultOptions, Error, ErrorKind, Options};
use serde::de::DeserializeOwned;
use std::io::Cursor;

/// Decode the repository's legacy bincode-1 fixed-integer format with an explicit byte budget.
///
/// `bincode::deserialize` is unlimited and accepts trailing bytes. The slice-based `Options`
/// decoder also disables its configured limit internally, so this helper deliberately uses the
/// reader API and verifies complete consumption itself.
pub fn deserialize_bounded<T: DeserializeOwned>(bytes: &[u8], max_bytes: u64) -> Result<T, Error> {
    let input_len = u64::try_from(bytes.len()).map_err(|_| Box::new(ErrorKind::SizeLimit))?;
    if input_len > max_bytes {
        return Err(Box::new(ErrorKind::SizeLimit));
    }

    let mut reader = Cursor::new(bytes);
    let value = DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(max_bytes)
        .deserialize_from(&mut reader)?;
    if reader.position() != input_len {
        return Err(Box::new(ErrorKind::Custom(format!(
            "trailing bytes after bincode value: consumed {}, input {input_len}",
            reader.position()
        ))));
    }
    Ok(value)
}

/// Decode one exact bincode value while using its encoded length as the deserialization budget.
pub fn deserialize_exact<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    let input_len = u64::try_from(bytes.len()).map_err(|_| Box::new(ErrorKind::SizeLimit))?;
    deserialize_bounded(bytes, input_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_legacy_fixed_integer_format() {
        let value = vec![1_u64, 2, 3];
        let bytes = bincode::serialize(&value).unwrap();
        assert_eq!(
            deserialize_bounded::<Vec<u64>>(&bytes, bytes.len() as u64).unwrap(),
            value
        );
    }

    #[test]
    fn rejects_input_over_budget() {
        let bytes = bincode::serialize(&vec![1_u8; 32]).unwrap();
        assert!(matches!(
            deserialize_bounded::<Vec<u8>>(&bytes, 16),
            Err(error) if matches!(*error, ErrorKind::SizeLimit)
        ));
    }

    #[test]
    fn rejects_forged_collection_length() {
        let bytes = u64::MAX.to_le_bytes();
        assert!(deserialize_bounded::<Vec<u8>>(&bytes, 1024).is_err());
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = bincode::serialize(&7_u64).unwrap();
        bytes.push(0);
        let error = deserialize_bounded::<u64>(&bytes, bytes.len() as u64).unwrap_err();
        assert!(error.to_string().contains("trailing bytes"));
    }
}
