use std::sync::Arc;

use aries_vcx::aries_vcx_core::WalletHandle;
use aries_vcx::aries_vcx_core::anoncreds::base_anoncreds::BaseAnonCreds;
use aries_vcx::aries_vcx_core::indy::wallet::{create_and_open_wallet, WalletConfig};
use aries_vcx::aries_vcx_core::ledger::base_ledger::{IndyLedgerRead, IndyLedgerWrite, AnoncredsLedgerRead, AnoncredsLedgerWrite, TxnAuthrAgrmtOptions};
use aries_vcx::aries_vcx_core::wallet::base_wallet::BaseWallet;
// use aries_vcx::aries_vcx_core::PoolHandle;
use aries_vcx::core::profile::{profile::Profile};
use aries_vcx::errors::error::VcxResult;

use crate::{errors::error::VcxUniFFIResult, runtime::block_on};

pub struct ProfileHolder {
    pub inner: Arc<dyn Profile>,
}

impl ProfileHolder {}

pub fn new_indy_profile(wallet_config: WalletConfig) -> VcxUniFFIResult<Arc<ProfileHolder>> {
    block_on(async {
        let wh = create_and_open_wallet(&wallet_config).await?;
        let inner: Arc<dyn Profile> = Arc::new(DummyProfile(wh));

        Ok(Arc::new(ProfileHolder {
            inner,
        }))
    })
}

#[derive(Debug,Clone)]
pub struct DummyProfile(WalletHandle);

impl Profile for DummyProfile {
    fn inject_indy_ledger_read(&self) -> Arc<dyn IndyLedgerRead>  {
        todo!()
    }

    fn inject_indy_ledger_write(&self) -> Arc<dyn IndyLedgerWrite>  {
        todo!()
    }

    fn inject_anoncreds(&self) -> Arc<dyn BaseAnonCreds>  {
        todo!()
    }

    fn inject_anoncreds_ledger_read(&self) -> Arc<dyn AnoncredsLedgerRead>  {
        todo!()
    }

    fn inject_anoncreds_ledger_write(&self) -> Arc<dyn AnoncredsLedgerWrite>  {
        todo!()
    }

    fn inject_wallet(&self) -> Arc<dyn BaseWallet>  {
        todo!()
    }

    fn update_taa_configuration(&self,taa_options:TxnAuthrAgrmtOptions) -> VcxResult<()>  {
        todo!()
    }
}
