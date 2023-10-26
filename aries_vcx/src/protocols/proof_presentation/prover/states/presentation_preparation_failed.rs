use std::{collections::HashMap, sync::Arc};

use messages::msg_fields::protocols::{present_proof::request::RequestPresentation, report_problem::ProblemReport};

use crate::errors::error::AriesVcxError;
use crate::errors::error::AriesVcxErrorKind;
use crate::{
    common::proofs::prover::prover::generate_indy_proof,
    core::profile::profile::Profile,
    errors::error::VcxResult,
    handlers::{
        proof_presentation::types::SelectedCredentials,
        util::{get_attach_as_string, Status},
    },
    protocols::proof_presentation::prover::states::finished::FinishedState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentationPreparationFailedState {
    pub presentation_request: RequestPresentation,
    pub problem_report: ProblemReport,
}

impl PresentationPreparationFailedState {
    pub async fn build_presentation(
        &self,
        profile: &Arc<dyn Profile>,
        credentials: &SelectedCredentials,
        self_attested_attrs: &HashMap<String, String>,
    ) -> VcxResult<String> {
        let proof_req_data_json =
            get_attach_as_string!(&self.presentation_request.content.request_presentations_attach);

        generate_indy_proof(profile, credentials, self_attested_attrs, &proof_req_data_json).await
    }
}

impl From<PresentationPreparationFailedState> for FinishedState {
    fn from(state: PresentationPreparationFailedState) -> Self {
        trace!("transit state from PresentationPreparationFailedState to FinishedState");
        FinishedState {
            presentation_request: Some(state.presentation_request),
            presentation: None,
            status: Status::Failed(state.problem_report),
        }
    }
}
