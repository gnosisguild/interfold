// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::sled_utils::{clear_all_caches, get_or_open_db_tree};
use anyhow::{Context, Result};
use e3_events::{Get, Insert, Remove};
use sled::{
    transaction::{ConflictableTransactionError, TransactionError},
    Tree,
};
use std::path::PathBuf;

pub struct SledDb {
    db: Tree,
}

impl SledDb {
    pub fn new(path: &PathBuf, tree: &str) -> Result<Self> {
        let db = get_or_open_db_tree(path, tree)?;
        Ok(Self { db })
    }

    pub fn close_all_connections() {
        clear_all_caches()
    }

    pub fn insert(&mut self, msg: Insert) -> Result<()> {
        self.db
            .insert(msg.key(), msg.value().to_vec())
            .context("Could not insert data into db")?;

        Ok(())
    }

    pub fn insert_batch(&mut self, msgs: &Vec<Insert>) -> Result<()> {
        self.db
            .transaction(|tx_db| {
                for msg in msgs {
                    tx_db.insert(msg.key().as_slice(), msg.value().to_vec())?;
                }
                Ok::<(), ConflictableTransactionError>(())
            })
            .context("Could not insert batch data into db")?;
        Ok(())
    }

    /// Atomically insert `msgs` only if none of their keys exist.
    pub fn insert_batch_if_absent(&mut self, msgs: &[Insert]) -> Result<bool> {
        let result = self.db.transaction(|tx_db| {
            for msg in msgs {
                if tx_db.get(msg.key().as_slice())?.is_some() {
                    return Err(ConflictableTransactionError::Abort(()));
                }
            }
            for msg in msgs {
                tx_db.insert(msg.key().as_slice(), msg.value().to_vec())?;
            }
            Ok::<(), ConflictableTransactionError<()>>(())
        });

        match result {
            Ok(()) => Ok(true),
            Err(TransactionError::Abort(())) => Ok(false),
            Err(TransactionError::Storage(error)) => {
                Err(error).context("Could not conditionally insert batch into db")
            }
        }
    }

    pub fn remove(&mut self, msg: Remove) -> Result<()> {
        self.db
            .remove(msg.key())
            .context("Could not remove data from db")?;
        Ok(())
    }

    pub fn get(&self, event: Get) -> Result<Option<Vec<u8>>> {
        let key = event.key();
        let str_key = String::from_utf8_lossy(key).into_owned();
        let res = self
            .db
            .get(key)
            .context(format!("Failed to fetch {}", str_key))?;
        Ok(res.map(|v| v.to_vec()))
    }

    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }

    /// Return whether the tree contains exactly the supplied keys and no others.
    pub fn has_exact_keys(&self, keys: &[Vec<u8>]) -> Result<bool> {
        let mut expected = keys.to_vec();
        expected.sort();
        expected.dedup();

        // `Tree::len` walks the full tree. Schema preflight must remain bounded even when an
        // unversioned store is unexpectedly large, so inspect at most one key beyond the allowed
        // set and fail closed on any iterator error.
        let observed = self
            .db
            .iter()
            .keys()
            .take(expected.len() + 1)
            .collect::<sled::Result<Vec<_>>>()?;
        Ok(observed.len() == expected.len()
            && observed
                .iter()
                .zip(expected)
                .all(|(observed, expected)| observed.as_ref() == expected.as_slice()))
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sled_db_caching() -> Result<()> {
        use tempfile::tempdir;

        // Section 1: Test basic cache functionality
        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let db_path = temp_dir.path().join("test_cache.db");

        // Create first instance and insert data
        let mut db1 = SledDb::new(&db_path, "datastore")?;
        db1.insert(Insert::new(b"test_key".to_vec(), b"test_value".to_vec()))?;

        // Create second instance to same path and verify data access
        let mut db2 = SledDb::new(&db_path, "datastore")?;
        let result = db2.get(Get::new(b"test_key".to_vec()))?;
        assert_eq!(
            result.unwrap(),
            b"test_value".to_vec(),
            "Values from db2 should match"
        );

        // Cross-modify and verify (db1 writes, db2 reads)
        db1.insert(Insert::new(b"key2".to_vec(), b"value2".to_vec()))?;
        assert_eq!(
            db2.get(Get::new(b"key2".to_vec()))?.unwrap(),
            b"value2".to_vec(),
            "db2 should see changes from db1"
        );

        // Section 2: Test cross-instance operations (db2 writes, db1 reads)
        db2.insert(Insert::new(b"key3".to_vec(), b"value3".to_vec()))?;
        assert_eq!(
            db1.get(Get::new(b"key3".to_vec()))?.unwrap(),
            b"value3".to_vec(),
            "db1 should see changes from db2"
        );

        // Section 3: Test cache with different path
        let second_path = temp_dir.path().join("different_cache.db");
        let mut db3 = SledDb::new(&second_path, "datastore")?;
        db3.insert(Insert::new(b"db3_key".to_vec(), b"db3_value".to_vec()))?;

        // Create another instance to the second path
        let db4 = SledDb::new(&second_path, "datastore")?;
        assert_eq!(
            db4.get(Get::new(b"db3_key".to_vec()))?.unwrap(),
            b"db3_value".to_vec(),
            "db4 should see db3's data"
        );

        // Verify first path data isn't in second path
        assert!(
            db4.get(Get::new(b"test_key".to_vec()))?.is_none(),
            "db4 should not see data from db1/db2"
        );

        // Verify second path data isn't in first path
        assert!(
            db1.get(Get::new(b"db3_key".to_vec()))?.is_none(),
            "db1 should not see data from db3/db4"
        );

        Ok(())
    }

    #[test]
    fn test_sled_db_batch_insert() -> Result<()> {
        use tempfile::tempdir;

        let temp_dir = tempdir().expect("Failed to create temporary directory");
        let db_path = temp_dir.path().join("test_batch.db");

        let mut db = SledDb::new(&db_path, "datastore")?;

        // Create a batch of inserts
        let batch = vec![
            Insert::new(b"batch_key1".to_vec(), b"batch_value1".to_vec()),
            Insert::new(b"batch_key2".to_vec(), b"batch_value2".to_vec()),
            Insert::new(b"batch_key3".to_vec(), b"batch_value3".to_vec()),
        ];

        // Insert the batch
        db.insert_batch(&batch)?;

        // Verify all items were inserted
        assert_eq!(
            db.get(Get::new(b"batch_key1".to_vec()))?.unwrap(),
            b"batch_value1".to_vec(),
            "First batch item should be retrievable"
        );
        assert_eq!(
            db.get(Get::new(b"batch_key2".to_vec()))?.unwrap(),
            b"batch_value2".to_vec(),
            "Second batch item should be retrievable"
        );
        assert_eq!(
            db.get(Get::new(b"batch_key3".to_vec()))?.unwrap(),
            b"batch_value3".to_vec(),
            "Third batch item should be retrievable"
        );

        // Verify non-existent key returns None
        assert!(
            db.get(Get::new(b"nonexistent".to_vec()))?.is_none(),
            "Non-existent key should return None"
        );

        Ok(())
    }

    #[test]
    fn conditional_batch_insert_never_overwrites_a_partial_key_set() -> Result<()> {
        use tempfile::tempdir;

        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test_conditional_batch.db");
        let mut db = SledDb::new(&db_path, "datastore")?;
        db.insert(Insert::new("existing", b"original".to_vec()))?;

        let inserted = db.insert_batch_if_absent(&[
            Insert::new("existing", b"replacement".to_vec()),
            Insert::new("missing", b"new".to_vec()),
        ])?;

        assert!(!inserted);
        assert_eq!(db.get(Get::new("existing"))?, Some(b"original".to_vec()));
        assert_eq!(db.get(Get::new("missing"))?, None);
        Ok(())
    }

    #[test]
    fn exact_key_match_rejects_missing_and_extra_keys() -> Result<()> {
        use tempfile::tempdir;

        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test_exact_keys.db");
        let mut db = SledDb::new(&db_path, "datastore")?;
        db.insert(Insert::new("identity-a", b"1".to_vec()))?;
        db.insert(Insert::new("identity-b", b"2".to_vec()))?;

        assert!(db.has_exact_keys(&[b"identity-a".to_vec(), b"identity-b".to_vec()])?);
        assert!(!db.has_exact_keys(&[b"identity-a".to_vec()])?);

        db.insert(Insert::new("protocol-state", b"3".to_vec()))?;
        assert!(!db.has_exact_keys(&[b"identity-a".to_vec(), b"identity-b".to_vec()])?);
        Ok(())
    }
}
