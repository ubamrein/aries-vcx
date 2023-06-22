// Copyright (c) 2023 Ubique Innovation AG <https://www.ubique.ch>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::{Arc, Mutex};

use aries_vcx::{
    common::proofs::proof_request::PresentationRequestData, errors::error::AriesVcxError,
    handlers::proof_presentation::verifier::Verifier,
    messages::msg_fields::protocols::present_proof::present::Presentation,
    protocols::proof_presentation::verifier::verification_status::PresentationVerificationStatus,
};
use serde_json::Value;

use crate::{
    core::profile::ProfileHolder,
    errors::error::{VcxUniFFIError, VcxUniFFIResult},
    handlers::connection::connection::Connection,
    runtime::block_on,
};

pub struct Verify {
    verifier: Mutex<Verifier>,
}

impl Verify {
    pub fn create_from_request(source_id: String, request: String) -> VcxUniFFIResult<Self> {
        let presentation_request: PresentationRequestData = serde_json::from_str(&request)?;
        let verifier = Mutex::new(Verifier::create_from_request(source_id, &presentation_request)?);
        Ok(Self { verifier })
    }
    pub fn send_request(&self, profile: Arc<ProfileHolder>, connection: Arc<Connection>) -> VcxUniFFIResult<()> {
        let mut guard = self.verifier.lock()?;
        let mut verifier = guard.clone();
        let m = block_on(async move {
            let send_message = connection.send_message(profile);
            verifier.send_presentation_request(send_message).await?;
            Ok::<_, AriesVcxError>(verifier)
        })?;
        *guard = m;
        Ok(())
    }
    pub fn verify(
        &self,
        profile: Arc<ProfileHolder>,
        connection: Arc<Connection>,
        presentation: String,
    ) -> VcxUniFFIResult<bool> {
        let mut guard = self.verifier.lock()?;
        let presentation: Presentation = serde_json::from_str(&presentation)?;
        let mut verifier = guard.clone();
        let m = block_on(async move {
            let send_message = connection.send_message(profile.clone());
            verifier
                .verify_presentation(&profile.inner, presentation, send_message)
                .await?;
            Ok::<_, AriesVcxError>(verifier)
        })?;
        *guard = m;
        let status = guard.get_verification_status();
        Ok(matches!(status, PresentationVerificationStatus::Valid))
    }
    pub fn get_revealed_attr(&self) -> VcxUniFFIResult<Vec<RevealedAttribute>> {
        let guard = self.verifier.lock()?;
        let proof: Value = serde_json::from_str(&guard.get_presentation_attachment()?)?;
        let Some(requested_proof) = proof["requested_proof"]["revealed_attrs"].as_object() else {
            return Err(
                VcxUniFFIError::InternalError {
            error_msg: "attachment-json has no requested_proof".to_string(),
        }
            )
        };
        let mut revealed_attributes = vec![];
        let state: Value = serde_json::from_str(&guard.get_presentation_request_attachment()?)?;
        let Some(requested_attributes) = state["requested_attributes"].as_object() else {
             return Err(
                VcxUniFFIError::InternalError {
            error_msg: "attachment-json has no requested_proof".to_string(),
        }
            )
        };
        for (referent, rp) in requested_proof {
            let req_attr = &requested_attributes[referent];
            let name = req_attr["name"].as_str().unwrap().to_string();
            let value = rp["raw"].as_str().unwrap().to_string();
            let encoded = rp["encoded"].as_str().unwrap().to_string();
            revealed_attributes.push(RevealedAttribute { name, value, encoded })
        }
        Ok(revealed_attributes)
    }
}

pub struct RevealedAttribute {
    pub name: String,
    pub value: String,
    pub encoded: String,
}
