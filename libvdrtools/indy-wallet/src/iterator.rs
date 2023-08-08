use std::sync::Arc;

use indy_api_types::errors::IndyError;

use crate::SecureEnclaveProvider;

use super::{
    encryption::decrypt_storage_record, storage::StorageIterator, wallet::Keys, WalletRecord,
};

pub(super) struct WalletIterator {
    storage_iterator: Box<dyn StorageIterator>,
    keys: Arc<Keys>,
    secure_enclave_provider: Option<Arc<dyn SecureEnclaveProvider>>
}

impl WalletIterator {
    pub fn new(storage_iter: Box<dyn StorageIterator>, keys: Arc<Keys>, secure_enclave_provider: Option<Arc<dyn SecureEnclaveProvider>> ) -> Self {
        WalletIterator {
            storage_iterator: storage_iter,
            keys,
            secure_enclave_provider
        }
    }

    pub async fn next(&mut self) -> Result<Option<WalletRecord>, IndyError> {
        let next_storage_entity = self.storage_iterator.next().await?;

        if let Some(next_storage_entity) = next_storage_entity {
            Ok(Some(decrypt_storage_record(
                &next_storage_entity,
                &self.keys,
                self.secure_enclave_provider.clone()
            )?))
        } else {
            Ok(None)
        }
    }

    pub fn get_total_count(&self) -> Result<Option<usize>, IndyError> {
        self.storage_iterator.get_total_count()
    }
}
