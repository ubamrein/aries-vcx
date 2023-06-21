// Copyright (c) 2023 Ubique Innovation AG <https://www.ubique.ch>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::{Mutex, Arc};

use aries_vcx::{
    common::proofs::proof_request::PresentationRequestData, handlers::proof_presentation::verifier::Verifier,
    messages::msg_fields::protocols::present_proof::present::Presentation,
    protocols::proof_presentation::verifier::verification_status::PresentationVerificationStatus, errors::error::AriesVcxError,
};

use crate::{
    core::profile::ProfileHolder, errors::error::VcxUniFFIResult, handlers::connection::connection::Connection,
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
            Ok::<_,AriesVcxError>(verifier)
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
            verifier.verify_presentation(&profile.inner, presentation, send_message).await?;
            Ok::<_, AriesVcxError>(verifier)
        })?;
        *guard = m;
        let status = guard.get_verification_status();
        Ok(matches!(status, PresentationVerificationStatus::Valid))
    }
}
