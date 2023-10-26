use std::any::Any;
use std::error::Error;
use std::sync::Arc;

use aries_vcx::aries_vcx_core::anoncreds::base_anoncreds::BaseAnonCreds;
use aries_vcx::aries_vcx_core::anoncreds::indy_anoncreds::IndySdkAnonCreds;
use aries_vcx::aries_vcx_core::errors::error::{AriesVcxCoreError, AriesVcxCoreErrorKind, VcxCoreResult};
use aries_vcx::aries_vcx_core::indy::wallet::{
    close_wallet, create_wallet_with_master_secret, open_wallet, WalletConfig,
};
use aries_vcx::aries_vcx_core::ledger::base_ledger::{
    AnoncredsLedgerRead, AnoncredsLedgerWrite, IndyLedgerRead, IndyLedgerWrite, TxnAuthrAgrmtOptions,
};
use aries_vcx::aries_vcx_core::wallet::base_wallet::BaseWallet;
use aries_vcx::aries_vcx_core::wallet::indy_wallet::IndySdkWallet;
use aries_vcx::aries_vcx_core::WalletHandle;
// use aries_vcx::aries_vcx_core::PoolHandle;
use aries_vcx::core::profile::profile::Profile;
use aries_vcx::errors::error::VcxResult;
use async_trait::async_trait;

use serde_json::Value;
use vdrtools::types::errors::IndyResult;
use vdrtools::types::IndyError;
use vdrtools::SecureEnclaveProvider;

use crate::errors::error::{CryptoError, VcxUniFFIError};
use crate::handlers::TypeMessage;
use crate::{errors::error::VcxUniFFIResult, runtime::block_on};
use aries_vcx::transport::Transport;

use super::http_client::NativeClient;

pub trait NativeCryptoProvider: Send + Sync {
    fn encrypt(&self, data: Vec<u8>, key_handle: String) -> Result<Vec<u8>, CryptoError>;
    fn decrypt(&self, data: Vec<u8>, key_handle: String) -> Result<Vec<u8>, CryptoError>;
    fn new_key(&self) -> Result<String, CryptoError>;
    fn get_handle(&self, ty: String, name: String, etype: Vec<u8>, ename: Vec<u8>) -> Result<String, CryptoError>;
}

pub struct NativeSecureEnclaveProvider {
    pub inner: Box<dyn NativeCryptoProvider>,
}

impl NativeSecureEnclaveProvider {
    pub fn new(provider: Box<dyn NativeCryptoProvider>) -> Self {
        Self { inner: provider }
    }
}

impl SecureEnclaveProvider for NativeSecureEnclaveProvider {
    fn encrypt(&self, data: &[u8], key_handle: &str) -> IndyResult<Vec<u8>> {
        self.inner.encrypt(data.to_vec(), key_handle.to_string()).map_err(|e| {
            IndyError::from_msg(
                vdrtools::types::errors::IndyErrorKind::WalletEncryptionError,
                "could not encrypt",
            )
        })
    }

    fn decrypt(&self, encrypted_data: &[u8], key_handle: &str) -> IndyResult<Vec<u8>> {
        self.inner
            .decrypt(encrypted_data.to_vec(), key_handle.to_string())
            .map_err(|e| {
                IndyError::from_msg(
                    vdrtools::types::errors::IndyErrorKind::WalletEncryptionError,
                    "could not decrypt",
                )
            })
    }

    fn new_key(&self) -> IndyResult<String> {
        self.inner.new_key().map_err(|e| {
            IndyError::from_msg(
                vdrtools::types::errors::IndyErrorKind::WalletEncryptionError,
                "could not create key",
            )
        })
    }

    fn get_handle(&self, ty: &str, name: &str, etype: &[u8], ename: &[u8]) -> IndyResult<String> {
        self.inner
            .get_handle(ty.into(), name.into(), etype.into(), ename.into())
            .map_err(|e| {
                IndyError::from_msg(
                    vdrtools::types::errors::IndyErrorKind::WalletEncryptionError,
                    "could not get handle",
                )
            })
    }
}

pub struct ProfileHolder {
    pub inner: Arc<dyn Profile>,
    pub transport: Arc<dyn Transport>,
    wallet_handle: WalletHandle,
}

impl ProfileHolder {
    pub fn get_credentials(&self) -> VcxUniFFIResult<String> {
        let w = self.inner.inject_anoncreds();
        let list = block_on(async { w.prover_get_credentials(None).await })?;
        Ok(list)
    }
    pub fn delete_credential(&self, id: String) -> VcxUniFFIResult<()> {
        let w = self.inner.inject_anoncreds();
        block_on(async { w.prover_delete_credential(&id).await })?;
        Ok(())
    }
    pub fn unpack_msg(&self, msg: String) -> VcxUniFFIResult<TypeMessage> {
        let w = self.inner.inject_wallet();
        let decrypted_package = block_on(w.unpack_message(msg.as_bytes()))?;
        let decrypted_package =
            std::str::from_utf8(&decrypted_package).map_err(|e| VcxUniFFIError::SerializationError {
                error_msg: format!("Wrong encoding {e}"),
            })?;
        let decrypted_package: Value = serde_json::from_str(decrypted_package)?;
        let msg = decrypted_package
            .get("message")
            .ok_or_else(|| VcxUniFFIError::SerializationError {
                error_msg: "Message not found".to_string(),
            })?
            .as_str()
            .ok_or_else(|| VcxUniFFIError::SerializationError {
                error_msg: "Message not a string".to_string(),
            })?;
        let kid = decrypted_package
            .get("recipient_verkey")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let mut deserialized_value = serde_json::from_str::<Value>(msg)?;

        let ty = deserialized_value
            .get("@type")
            .unwrap_or(&Value::Null)
            .as_str()
            .unwrap_or_default()
            .to_string();
        if let Some(t) = deserialized_value.get_mut("~thread") {
            if t.get("thid").is_none() {
                *t = Value::Null;
            }
        }
        let content = serde_json::to_string(&deserialized_value).unwrap();
        Ok(TypeMessage { kid, ty, content })
    }
}
impl Drop for ProfileHolder {
    fn drop(&mut self) {
        println!("Finalizing ProfileHolder");
        let _ = block_on(async move { close_wallet(self.wallet_handle).await });
    }
}

pub fn new_indy_profile(
    wallet_config: WalletConfig,
    native_client: Arc<NativeClient>,
    native_secure_enclave_provider: Arc<NativeSecureEnclaveProvider>,
    ledger_base_url: String,
) -> VcxUniFFIResult<Arc<ProfileHolder>> {
    block_on(async {
        create_wallet_with_master_secret(&wallet_config).await?;
        let wh = open_wallet(&wallet_config, Some(native_secure_enclave_provider)).await?;
        let inner: Arc<dyn Profile> = Arc::new(DummyProfile(wh, ledger_base_url));
        let transport: Arc<dyn Transport> = native_client;

        Ok(Arc::new(ProfileHolder {
            inner,
            transport,
            wallet_handle: wh,
        }))
    })
}

#[derive(Debug, Clone)]
pub struct DummyProfile(WalletHandle, String);

impl Profile for DummyProfile {
    fn inject_indy_ledger_read(&self) -> Arc<dyn IndyLedgerRead> {
        todo!()
    }

    fn inject_indy_ledger_write(&self) -> Arc<dyn IndyLedgerWrite> {
        todo!()
    }

    fn inject_anoncreds(&self) -> Arc<dyn BaseAnonCreds> {
        let a: Arc<dyn BaseAnonCreds> = Arc::new(IndySdkAnonCreds::new(self.0));
        a
    }

    fn inject_anoncreds_ledger_read(&self) -> Arc<dyn AnoncredsLedgerRead> {
        let d: Arc<dyn AnoncredsLedgerRead> = Arc::new(DummyLedgerRead(self.1.clone()));
        d
    }

    fn inject_anoncreds_ledger_write(&self) -> Arc<dyn AnoncredsLedgerWrite> {
        todo!()
    }

    fn inject_wallet(&self) -> Arc<dyn BaseWallet> {
        let sdk_wallet = IndySdkWallet::new(self.0);
        Arc::new(sdk_wallet)
    }

    fn update_taa_configuration(&self, taa_options: TxnAuthrAgrmtOptions) -> VcxResult<()> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct DummyLedgerRead(String);

#[async_trait]
impl AnoncredsLedgerRead for DummyLedgerRead {
    async fn get_schema(&self, schema_id: &str, _submitter_did: Option<&str>) -> VcxCoreResult<String> {
        println!("{schema_id}");
        let res = serde_json::to_string(
            ureq::get(&format!("{}/schemas/{schema_id}", self.0))
                .call()
                .map_err(|e| AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, format!("{e}")))?
                .into_json::<serde_json::Value>()
                .map_err(|e| AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, format!("{e}")))?
                .get("schema")
                .ok_or_else(|| {
                    AriesVcxCoreError::from_msg(
                        AriesVcxCoreErrorKind::InvalidUrl,
                        "no cred def in response".to_string(),
                    )
                })?,
        )?;
        println!("{res}");
        VcxCoreResult::Ok(res)
    }
    async fn get_cred_def(&self, cred_def_id: &str, _submitter_did: Option<&str>) -> VcxCoreResult<String> {
        println!("{cred_def_id}");
        let res = serde_json::to_string(
            &ureq::get(&format!("{}/credential-definitions/{cred_def_id}", self.0))
                .call()
                .map_err(|e| AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, format!("{e}")))?
                .into_json::<serde_json::Value>()
                .map_err(|e| AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, format!("{e}")))?
                .get("credential_definition")
                .ok_or_else(|| {
                    AriesVcxCoreError::from_msg(
                        AriesVcxCoreErrorKind::InvalidUrl,
                        "no cred def in response".to_string(),
                    )
                })?,
        )
        .unwrap();
        println!("{res}");
        VcxCoreResult::Ok(res)
    }
    async fn get_rev_reg_def_json(&self, rev_reg_id: &str) -> VcxCoreResult<String> {
        println!("Revregid: {rev_reg_id}");
        let res = ureq::get(&format!("{}/rev_reg_def/{rev_reg_id}", self.0))
            .call()
            .map_err(|e| AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, format!("{e}")))?
            .into_json::<serde_json::Value>()
            .map_err(|e| AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, format!("{e}")))?
            .get("rev_reg_def")
            .ok_or_else(|| {
                AriesVcxCoreError::from_msg(AriesVcxCoreErrorKind::InvalidUrl, "no rev reg in response".to_string())
            })?.to_string();

        VcxCoreResult::Ok(res)
    }
    async fn get_rev_reg_delta_json(
        &self,
        rev_reg_id: &str,
        from: Option<u64>,
        to: Option<u64>,
    ) -> VcxCoreResult<(String, String, u64)> {
        todo! {}
    }
    async fn get_rev_reg(&self, rev_reg_id: &str, timestamp: u64) -> VcxCoreResult<(String, String, u64)> {
        todo! {}
    }
}
