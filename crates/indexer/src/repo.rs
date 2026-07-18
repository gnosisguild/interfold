// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::{models::E3, DataStore, SharedStore};
use eyre::Result;

pub struct E3Repository<S: DataStore> {
    store: SharedStore<S>,
    e3_id: u64,
}

impl<S: DataStore> E3Repository<S> {
    pub fn new(store: SharedStore<S>, e3_id: u64) -> Self {
        Self { store, e3_id }
    }

    pub async fn set_e3(&mut self, value: E3) -> Result<()> {
        let key = self.e3_key();
        self.store
            .insert(&key, &value)
            .await
            .map_err(|e| eyre::eyre!("Could not store E3 at '{key}' due to error: {e}"))?;
        Ok(())
    }

    pub async fn get_e3(&self) -> Result<E3> {
        let key = self.e3_key();
        let e3_crisp = self
            .store
            .get::<E3>(&key)
            .await
            .map_err(|e| eyre::eyre!("Could get crisp at '{key}' due to error: {e}"))?
            .ok_or(eyre::eyre!("No data found at {key}"))?;
        Ok(e3_crisp)
    }
    pub async fn insert_ciphertext_input(&mut self, data: Vec<u8>, index: u64) -> Result<()> {
        let key = self.e3_key();
        self.store
            .modify(&key, |e3_obj: Option<E3>| {
                e3_obj.map(|mut e| {
                    e.ciphertext_inputs.push((data.clone(), index));
                    e
                })
            })
            .await
            .map_err(|_| eyre::eyre!("Could not append ciphertext_input for '{key}'"))?;

        Ok(())
    }
    pub async fn set_plaintext_output(&mut self, data: Vec<u8>) -> Result<()> {
        let key = self.e3_key();
        self.store
            .modify(&key, |e3_obj: Option<E3>| {
                e3_obj.map(|mut e| {
                    e.plaintext_output = data.clone();
                    e
                })
            })
            .await
            .map_err(|_| eyre::eyre!("Could not append ciphertext_input for '{key}'"))?;
        Ok(())
    }

    pub async fn set_ciphertext_output(&mut self, data: Vec<u8>) -> Result<()> {
        let key = self.e3_key();
        self.store
            .modify(&key, |e3_obj: Option<E3>| {
                e3_obj.map(|mut e| {
                    e.ciphertext_output = data.clone();
                    e
                })
            })
            .await
            .map_err(|_| eyre::eyre!("Could not append ciphertext_input for '{key}'"))?;
        Ok(())
    }

    pub async fn set_ciphertext_commitment(&mut self, data: Vec<u8>) -> Result<()> {
        let key = self.e3_key();
        self.store
            .modify(&key, |e3_obj: Option<E3>| {
                e3_obj.map(|mut e| {
                    e.ciphertext_commitment = data.clone();
                    e
                })
            })
            .await
            .map_err(|_| eyre::eyre!("Could not set ciphertext_commitment for '{key}'"))?;
        Ok(())
    }

    fn e3_key(&self) -> String {
        let e3_id = self.e3_id;
        format!("_e3:{e3_id}")
    }
}
