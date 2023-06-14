// Copyright (c) 2023 Ubique Innovation AG <https://www.ubique.ch>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::{Arc, Mutex};

use aries_vcx::{
    errors::error::VcxResult,
    handlers::issuance::holder::Holder,
    messages::{
        msg_fields::protocols::cred_issuance::{issue_credential::IssueCredential, offer_credential::OfferCredential},
        AriesMessage,
    },
    protocols::issuance::holder::state_machine::HolderSM,
};

use crate::{
    core::profile::ProfileHolder, errors::error::VcxUniFFIResult, handlers::connection::connection::Connection,
    runtime::block_on,
};

pub struct Message {
    pub message: AriesMessage,
}

pub struct Issuance {
    handler: Mutex<Holder>,
    connection: Arc<Connection>,
}

pub fn create_vc_receiver(source_id: String, connection: Arc<Connection>) -> VcxUniFFIResult<Arc<Issuance>> {
    let handler = Mutex::new(Holder::create(&source_id).unwrap());
    Ok(Arc::new(Issuance { handler, connection }))
}

impl Issuance {
    pub fn receive_offer(&self, offer: String) -> VcxUniFFIResult<()> {
        let mut guard = self.handler.lock()?;
        let offer: OfferCredential = serde_json::from_str(&offer)?;
        let holder = Holder::create_from_offer(&guard.get_source_id(), offer).unwrap();
        *guard = holder;
        Ok(())
    }
    pub fn send_request(&self, profile: Arc<ProfileHolder>) -> VcxUniFFIResult<()> {
        let mut guard = self.handler.lock()?;
        let pw = self.connection.pairwise_info()?;
        let connection = self.connection.clone();
        let mut holder = guard.clone();
        // connection.send_message
        block_on(async {
            let send_message = connection.send_message(profile.clone());
            holder.send_request(&profile.inner, pw.pw_did, send_message).await?;
            *guard = holder;
            Ok(())
        })
    }
    pub fn process_credential(&self, profile: Arc<ProfileHolder>, credential: String) -> VcxUniFFIResult<()> {
        let mut guard = self.handler.lock()?;
        let credential: IssueCredential = serde_json::from_str(&credential)?;
        let connection = self.connection.clone();
        let mut holder = guard.clone();

        block_on(async {
            let send_message = connection.send_message(profile.clone());
            holder.process_credential(&profile.inner, credential, send_message).await?;
            *guard = holder;
            Ok(())
        })
    }
    pub fn get_credential(&self) -> VcxUniFFIResult<CredentialEntry> {
        let guard = self.handler.lock()?;
        let (credential_id, cred) = guard.get_credential()?;
        let credential = serde_json::to_string(&cred)?;
        Ok(CredentialEntry {
            credential_id, 
            credential
        })
    }
}

pub struct CredentialEntry {
    pub credential_id: String,
    pub credential: String
}

// pub fn _send_message(
//     w: WalletHandle,
//     connection: Connection<
//         aries_vcx::protocols::connection::initiation_type::Invitee,
//         aries_vcx::protocols::connection::invitee::states::completed::Completed,
//     >,
// ) -> SendClosure {
//     Box::new(move |m: AriesMessage| {
//         println!("{m:?}");

//         Box::pin(async move {
//             let client = HttpClient;
//             let wallet: Arc<dyn BaseWallet> = Arc::new(IndySdkWallet::new(w));
//             connection.send_message(&wallet, &m, &client).await.unwrap();
//             VcxResult::Ok(())
//         })
//     })
// }
