// Copyright (c) 2023 Ubique Innovation AG <https://www.ubique.ch>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use aries_vcx::{
    errors::error::AriesVcxError,
    handlers::proof_presentation::{
        prover::Prover,
        types::{
            RetrievedCredentials, SelectedCredentialForReferent, SelectedCredentialForReferentCredential,
            SelectedCredentialInfo, SelectedCredentials,
        },
    },
    messages::msg_fields::protocols::present_proof::request::RequestPresentation,
};

use crate::{
    core::profile::ProfileHolder, errors::error::VcxUniFFIResult, handlers::connection::connection::Connection,
    runtime::block_on,
};

pub struct Proof {
    handler: Mutex<Prover>,
}

impl Proof {
    pub fn create_from_request(source_id: String, presentation_request: String) -> VcxUniFFIResult<Proof> {
        let req: RequestPresentation = serde_json::from_str(&presentation_request)?;
        let p = Prover::create_from_request(&source_id, req)?;
        let handler = Mutex::new(p);
        Ok(Proof { handler })
    }
    pub fn select_credentials(&self, profile: Arc<ProfileHolder>) -> VcxUniFFIResult<String> {
        let guard = self.handler.lock()?;
        let prove = guard.clone();
        let creds = block_on(async move {
            let creds = prove.retrieve_credentials(&profile.inner).await?;
            Ok::<_, AriesVcxError>(creds)
        })?;
        log::debug!("{:?}", creds);
        Ok(serde_json::to_string(&creds).unwrap())
    }
    pub fn choose_credentials(&self, selected_credentials: String) -> VcxUniFFIResult<String> {
        let creds: RetrievedCredentials = serde_json::from_str(&selected_credentials)?;
        let mut selected: SelectedCredentials = SelectedCredentials::default();
        for (referent, value) in creds.credentials_by_referent {
            if value.is_empty() {
                selected.credential_for_referent.insert(
                    referent.clone(),
                    SelectedCredentialForReferent {
                        credential: SelectedCredentialForReferentCredential {
                            cred_info: SelectedCredentialInfo {
                                referent,
                                schema_id: String::from(""),
                                cred_def_id: String::from(""),
                                rev_reg_id: None,
                                cred_rev_id: None,
                                revealed: Some(false),
                            },
                        },
                        tails_dir: None,
                    },
                );
                continue;
            }
            let first = value[0].clone();
            selected.select_credential_for_referent_from_retrieved(referent, first, None);
        }

        Ok(serde_json::to_string(&selected)?)
    }
    pub fn get_proof_attachment(&self) -> VcxUniFFIResult<String> {
        let guard = self.handler.lock()?;
        let attachment = guard.get_proof_request_attachment()?;
        Ok(attachment)
    }
    pub fn send_presentation(
        &self,
        profile: Arc<ProfileHolder>,
        connection: Arc<Connection>,
        selected_credentials: String,
    ) -> VcxUniFFIResult<()> {
        let guard = self.handler.lock()?;
        let mut prove = guard.clone();

        let credentials: SelectedCredentials = serde_json::from_str(&selected_credentials)?;
        let profile2 = profile.clone();
        let mut prove = block_on(async move {
            prove
                .generate_presentation(&profile.inner, credentials, HashMap::new())
                .await?;
            Ok::<_, AriesVcxError>(prove)
        })?;
        block_on(async move {
            let send_message = connection.send_message(profile2);
            prove.send_presentation(send_message).await
        })?;
        Ok(())
    }
}
