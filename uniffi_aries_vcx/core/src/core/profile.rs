use std::sync::Arc;

use aries_vcx::aries_vcx_core::anoncreds::base_anoncreds::BaseAnonCreds;
use aries_vcx::aries_vcx_core::anoncreds::indy_anoncreds::IndySdkAnonCreds;
use aries_vcx::aries_vcx_core::errors::error::VcxCoreResult;
use aries_vcx::aries_vcx_core::indy::wallet::{create_wallet_with_master_secret, open_wallet, WalletConfig};
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

use crate::{errors::error::VcxUniFFIResult, runtime::block_on};
use aries_vcx::transport::Transport;

use super::http_client::NativeClient;

pub struct ProfileHolder {
    pub inner: Arc<dyn Profile>,
    pub transport: Arc<dyn Transport>,
}

impl ProfileHolder {}

pub fn new_indy_profile(
    wallet_config: WalletConfig,
    native_client: Arc<NativeClient>,
) -> VcxUniFFIResult<Arc<ProfileHolder>> {
    block_on(async {
        create_wallet_with_master_secret(&wallet_config).await?;
        let wh = open_wallet(&wallet_config).await?;
        let inner: Arc<dyn Profile> = Arc::new(DummyProfile(wh));
        let transport: Arc<dyn Transport> = native_client;

        Ok(Arc::new(ProfileHolder { inner, transport }))
    })
}

#[derive(Debug, Clone)]
pub struct DummyProfile(WalletHandle);

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
        let d: Arc<dyn AnoncredsLedgerRead> = Arc::new(DummyLedgerRead);
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
struct DummyLedgerRead;

#[async_trait]
impl AnoncredsLedgerRead for DummyLedgerRead {
    async fn get_schema(&self, schema_id: &str, submitter_did: Option<&str>) -> VcxCoreResult<String> {
        todo! {}
    }
    async fn get_cred_def(&self, cred_def_id: &str, submitter_did: Option<&str>) -> VcxCoreResult<String> {
        let cred_def = include_str!("../../credDef2.json");
        VcxCoreResult::Ok(cred_def.to_string())
    }
    async fn get_rev_reg_def_json(&self, rev_reg_id: &str) -> VcxCoreResult<String> {
        let cred_def = include_str!("../../revregdef.json");
        VcxCoreResult::Ok(cred_def.to_string())
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
