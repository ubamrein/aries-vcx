use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::core::profile::profile::Profile;
use crate::errors::error::prelude::*;
use crate::handlers::proof_presentation::types::SelectedCredentials;
use crate::handlers::util::{
    make_attach_from_str, matches_opt_thread_id, matches_thread_id, AttachmentId, PresentationProposalData, Status,
};
use crate::protocols::common::build_problem_report_msg;
use crate::protocols::proof_presentation::prover::messages::ProverMessages;
use crate::protocols::proof_presentation::prover::states::finished::FinishedState;
use crate::protocols::proof_presentation::prover::states::initial::InitialProverState;
use crate::protocols::proof_presentation::prover::states::presentation_preparation_failed::PresentationPreparationFailedState;
use crate::protocols::proof_presentation::prover::states::presentation_prepared::PresentationPreparedState;
use crate::protocols::proof_presentation::prover::states::presentation_proposal_sent::PresentationProposalSent;
use crate::protocols::proof_presentation::prover::states::presentation_request_received::PresentationRequestReceived;
use crate::protocols::proof_presentation::prover::states::presentation_sent::PresentationSentState;
use crate::protocols::proof_presentation::prover::verify_thread_id;
use crate::protocols::SendClosure;

use chrono::Utc;
use messages::decorators::thread::Thread;
use messages::decorators::timing::Timing;
use messages::msg_fields::protocols::notification::Notification;
use messages::msg_fields::protocols::present_proof::ack::AckPresentation;
use messages::msg_fields::protocols::present_proof::present::{
    Presentation, PresentationContent, PresentationDecorators,
};
use messages::msg_fields::protocols::present_proof::propose::{
    PresentationPreview, ProposePresentation, ProposePresentationContent, ProposePresentationDecorators,
};
use messages::msg_fields::protocols::present_proof::request::RequestPresentation;
use messages::msg_fields::protocols::present_proof::PresentProof;
use messages::msg_fields::protocols::report_problem::ProblemReport;
use messages::AriesMessage;
use uuid::Uuid;

/// A state machine that tracks the evolution of states for a Prover during
/// the Present Proof protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProverSM {
    source_id: String,
    thread_id: String,
    state: ProverFullState,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProverState {
    Initial,
    PresentationProposalSent,
    PresentationRequestReceived,
    PresentationPrepared,
    PresentationPreparationFailed,
    PresentationSent,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProverFullState {
    Initial(InitialProverState),
    PresentationProposalSent(PresentationProposalSent),
    PresentationRequestReceived(PresentationRequestReceived),
    PresentationPrepared(PresentationPreparedState),
    PresentationPreparationFailed(PresentationPreparationFailedState),
    PresentationSent(PresentationSentState),
    Finished(FinishedState),
}

impl fmt::Display for ProverFullState {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            ProverFullState::Initial(_) => f.write_str("Initial"),
            ProverFullState::PresentationProposalSent(_) => f.write_str("PresentationProposalSent"),
            ProverFullState::PresentationRequestReceived(_) => f.write_str("PresentationRequestReceived"),
            ProverFullState::PresentationPrepared(_) => f.write_str("PresentationPrepared"),
            ProverFullState::PresentationPreparationFailed(_) => f.write_str("PresentationPreparationFailed"),
            ProverFullState::PresentationSent(_) => f.write_str("PresentationSent"),
            ProverFullState::Finished(_) => f.write_str("Finished"),
        }
    }
}

fn build_presentation_msg(thread_id: &str, presentation_attachment: String) -> VcxResult<Presentation> {
    let id = Uuid::new_v4().to_string();

    let content = PresentationContent::new(vec![make_attach_from_str!(
        &presentation_attachment,
        AttachmentId::Presentation.as_ref().to_string()
    )]);
    let mut decorators = PresentationDecorators::new(Thread::new(thread_id.to_owned()));
    let mut timing = Timing::default();
    timing.out_time = Some(Utc::now());
    decorators.timing = Some(timing);

    Ok(Presentation::with_decorators(id, content, decorators))
}

impl Default for ProverFullState {
    fn default() -> Self {
        Self::PresentationRequestReceived(PresentationRequestReceived::default())
    }
}

impl ProverSM {
    pub fn new(source_id: String) -> ProverSM {
        ProverSM {
            source_id,
            thread_id: Uuid::new_v4().to_string(),
            state: ProverFullState::Initial(InitialProverState {}),
        }
    }

    pub fn from_request(presentation_request: RequestPresentation, source_id: String) -> ProverSM {
        ProverSM {
            source_id,
            thread_id: presentation_request.id.clone(),
            state: ProverFullState::PresentationRequestReceived(PresentationRequestReceived { presentation_request }),
        }
    }

    pub async fn send_presentation_proposal(
        self,
        proposal_data: PresentationProposalData,
        send_message: SendClosure,
    ) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::Initial(_) => {
                let id = self.thread_id.clone();
                let preview = PresentationPreview::new(proposal_data.attributes, proposal_data.predicates);
                let mut content = ProposePresentationContent::new(preview);
                content.comment = proposal_data.comment;

                let decorators = ProposePresentationDecorators::default();

                let proposal = ProposePresentation::with_decorators(id, content, decorators);

                send_message(proposal.clone().into()).await?;
                ProverFullState::PresentationProposalSent(PresentationProposalSent::new(proposal))
            }
            ProverFullState::PresentationRequestReceived(_) => {
                let id = Uuid::new_v4().to_string();
                let preview = PresentationPreview::new(proposal_data.attributes, proposal_data.predicates);
                let mut content = ProposePresentationContent::new(preview);
                content.comment = proposal_data.comment;

                let mut decorators = ProposePresentationDecorators::default();
                decorators.thread = Some(Thread::new(self.thread_id.clone()));

                let proposal = ProposePresentation::with_decorators(id, content, decorators);

                send_message(proposal.clone().into()).await?;
                ProverFullState::PresentationProposalSent(PresentationProposalSent::new(proposal))
            }
            s => {
                warn!("Unable to send presentation proposal in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub async fn decline_presentation_request(self, reason: String, send_message: SendClosure) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::PresentationRequestReceived(state) => {
                let problem_report =
                    Self::_handle_reject_presentation_request(send_message, &reason, &self.thread_id).await?;
                ProverFullState::Finished((state, problem_report).into())
            }
            ProverFullState::PresentationPrepared(_) => {
                let problem_report =
                    Self::_handle_reject_presentation_request(send_message, &reason, &self.thread_id).await?;
                ProverFullState::Finished(FinishedState::declined(problem_report))
            }
            s => {
                warn!("Unable to decline presentation request in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub async fn negotiate_presentation(
        self,
        presentation_preview: PresentationPreview,
        send_message: SendClosure,
    ) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::PresentationRequestReceived(state) => {
                Self::_handle_presentation_proposal(send_message, presentation_preview, &self.thread_id).await?;
                ProverFullState::Finished(state.into())
            }
            ProverFullState::PresentationPrepared(state) => {
                Self::_handle_presentation_proposal(send_message, presentation_preview, &self.thread_id).await?;
                ProverFullState::Finished(state.into())
            }
            s => {
                warn!("Unable to send handle presentation proposal in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub async fn generate_presentation(
        self,
        profile: &Arc<dyn Profile>,
        credentials: SelectedCredentials,
        self_attested_attrs: HashMap<String, String>,
    ) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::PresentationRequestReceived(state) => {
                match state
                    .build_presentation(profile, &credentials, &self_attested_attrs)
                    .await
                {
                    Ok(presentation) => {
                        let presentation = build_presentation_msg(&self.thread_id, presentation)?;
                        ProverFullState::PresentationPrepared((state, presentation).into())
                    }
                    Err(err) => {
                        let problem_report = build_problem_report_msg(Some(err.to_string()), &self.thread_id);
                        error!(
                            "Failed bo build presentation, sending problem report: {:?}",
                            problem_report
                        );
                        ProverFullState::PresentationPreparationFailed((state, problem_report).into())
                    }
                }
            }
            s => {
                warn!("Unable to send generate presentation in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub fn set_presentation(self, mut presentation: Presentation) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::PresentationRequestReceived(state) => {
                presentation.decorators.thread.thid = self.thread_id.clone();

                ProverFullState::PresentationPrepared((state, presentation).into())
            }
            s => {
                warn!("Unable to send set presentation in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub fn receive_presentation_ack(self, ack: AckPresentation) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::PresentationSent(state) => ProverFullState::Finished((state, ack).into()),
            s => {
                warn!("Unable to process presentation ack in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub async fn send_presentation(self, send_message: SendClosure) -> VcxResult<Self> {
        let state = match self.state {
            ProverFullState::PresentationPrepared(state) => {
                send_message(state.presentation.clone().into()).await?;
                ProverFullState::PresentationSent((state).into())
            }
            ProverFullState::PresentationPreparationFailed(state) => {
                send_message(state.problem_report.clone().into()).await?;
                ProverFullState::Finished((state).into())
            }
            s => {
                warn!("Unable to send send presentation in state {}", s);
                s
            }
        };
        Ok(Self { state, ..self })
    }

    pub fn find_message_to_handle(&self, messages: HashMap<String, AriesMessage>) -> Option<(String, AriesMessage)> {
        trace!("Prover::find_message_to_handle >>> messages: {:?}", messages);
        for (uid, message) in messages {
            match self.state {
                ProverFullState::PresentationProposalSent(_) => match &message {
                    AriesMessage::ReportProblem(msg) => {
                        if matches_opt_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    AriesMessage::Notification(Notification::ProblemReport(msg)) => {
                        if matches_opt_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    AriesMessage::PresentProof(PresentProof::RequestPresentation(msg)) => {
                        if matches_opt_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    _ => {}
                },
                ProverFullState::PresentationSent(_) => match &message {
                    AriesMessage::Notification(Notification::Ack(msg)) => {
                        if matches_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    AriesMessage::PresentProof(PresentProof::Ack(msg)) => {
                        if matches_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    AriesMessage::ReportProblem(msg) => {
                        if matches_opt_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    AriesMessage::Notification(Notification::ProblemReport(msg)) => {
                        if matches_opt_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    AriesMessage::PresentProof(PresentProof::ProblemReport(msg)) => {
                        if matches_opt_thread_id!(msg, self.thread_id.as_str()) {
                            return Some((uid, message));
                        }
                    }
                    _ => {}
                },
                _ => {}
            };
        }
        None
    }

    pub async fn step(
        self,
        profile: &Arc<dyn Profile>,
        message: ProverMessages,
        send_message: Option<SendClosure>,
    ) -> VcxResult<ProverSM> {
        trace!("ProverSM::step >>> message: {:?}", message);
        verify_thread_id(&self.thread_id, &message)?;
        let prover_sm = match &self.state {
            ProverFullState::Initial(_) => match message {
                ProverMessages::PresentationProposalSend(proposal_data) => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.send_presentation_proposal(proposal_data, send_message).await?
                }
                _ => {
                    warn!("Unable to process received message in this state");
                    self
                }
            },
            ProverFullState::PresentationProposalSent(_) => {
                match message {
                    ProverMessages::PresentationRequestReceived(request) => {
                        let state =
                            ProverFullState::PresentationRequestReceived(PresentationRequestReceived::new(request));
                        ProverSM { state, ..self }
                    }
                    // TODO: Perhaps use a different message type?
                    ProverMessages::PresentationRejectReceived(problem_report) => {
                        let state = ProverFullState::Finished(FinishedState::declined(problem_report));
                        ProverSM { state, ..self }
                    }
                    _ => {
                        warn!("Unable to process received message in this state");
                        self
                    }
                }
            }
            ProverFullState::PresentationRequestReceived(_) => match message {
                ProverMessages::PresentationProposalSend(proposal_data) => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.send_presentation_proposal(proposal_data, send_message).await?
                }
                ProverMessages::SetPresentation(presentation) => self.set_presentation(presentation)?,
                ProverMessages::PreparePresentation((credentials, self_attested_attrs)) => {
                    self.generate_presentation(profile, credentials, self_attested_attrs)
                        .await?
                }
                ProverMessages::RejectPresentationRequest(reason) => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.decline_presentation_request(reason, send_message).await?
                }
                ProverMessages::ProposePresentation(preview) => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.negotiate_presentation(preview, send_message).await?
                }
                _ => {
                    warn!("Unable to process received message in this state");
                    self
                }
            },
            ProverFullState::PresentationPrepared(_) => match message {
                ProverMessages::SendPresentation => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.send_presentation(send_message).await?
                }
                ProverMessages::RejectPresentationRequest(reason) => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.decline_presentation_request(reason, send_message).await?
                }
                ProverMessages::ProposePresentation(preview) => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.negotiate_presentation(preview, send_message).await?
                }
                _ => {
                    warn!("Unable to process received message in this state");
                    self
                }
            },
            ProverFullState::PresentationPreparationFailed(_) => match message {
                ProverMessages::SendPresentation => {
                    let send_message = send_message.ok_or(AriesVcxError::from_msg(
                        AriesVcxErrorKind::InvalidState,
                        "Attempted to call undefined send_message callback",
                    ))?;
                    self.send_presentation(send_message).await?
                }
                _ => {
                    warn!("Unable to process received message in this state");
                    self
                }
            },
            ProverFullState::PresentationSent(state) => match message {
                ProverMessages::PresentationAckReceived(ack) => {
                    let state = ProverFullState::Finished((state.clone(), ack).into());
                    ProverSM { state, ..self }
                }
                ProverMessages::PresentationRejectReceived(problem_report) => {
                    let state = ProverFullState::Finished((state.clone(), problem_report).into());
                    ProverSM { state, ..self }
                }
                ProverMessages::RejectPresentationRequest(_) => {
                    return Err(AriesVcxError::from_msg(
                        AriesVcxErrorKind::ActionNotSupported,
                        "Presentation is already sent",
                    ));
                }
                _ => {
                    warn!("Unable to process received message in this state");
                    self
                }
            },
            ProverFullState::Finished(_) => self,
        };
        Ok(prover_sm)
    }

    async fn _handle_reject_presentation_request<'a>(
        send_message: SendClosure,
        reason: &'a str,
        thread_id: &'a str,
    ) -> VcxResult<ProblemReport> {
        let problem_report = build_problem_report_msg(Some(reason.to_string()), thread_id);
        send_message(problem_report.clone().into()).await?;
        Ok(problem_report)
    }

    async fn _handle_presentation_proposal(
        send_message: SendClosure,
        preview: PresentationPreview,
        thread_id: &str,
    ) -> VcxResult<()> {
        let id = Uuid::new_v4().to_string();
        let content = ProposePresentationContent::new(preview);
        let mut decorators = ProposePresentationDecorators::default();
        let thread = Thread::new(thread_id.to_owned());
        let mut timing = Timing::default();
        timing.out_time = Some(Utc::now());
        decorators.thread = Some(thread);
        decorators.timing = Some(timing);

        let proposal = ProposePresentation::with_decorators(id, content, decorators);
        send_message(proposal.into()).await
    }

    pub fn source_id(&self) -> String {
        self.source_id.clone()
    }

    pub fn get_thread_id(&self) -> VcxResult<String> {
        Ok(self.thread_id.clone())
    }

    pub fn get_state(&self) -> ProverState {
        match self.state {
            ProverFullState::Initial(_) => ProverState::Initial,
            ProverFullState::PresentationProposalSent(_) => ProverState::PresentationProposalSent,
            ProverFullState::PresentationRequestReceived(_) => ProverState::PresentationRequestReceived,
            ProverFullState::PresentationPrepared(_) => ProverState::PresentationPrepared,
            ProverFullState::PresentationPreparationFailed(_) => ProverState::PresentationPreparationFailed,
            ProverFullState::PresentationSent(_) => ProverState::PresentationSent,
            ProverFullState::Finished(ref status) => match status.status {
                Status::Success => ProverState::Finished,
                _ => ProverState::Failed,
            },
        }
    }

    pub fn progressable_by_message(&self) -> bool {
        trace!("Prover::states::progressable_by_message >> state: {:?}", self.state);
        match self.state {
            ProverFullState::Initial(_) => false,
            ProverFullState::PresentationProposalSent(_) => true,
            ProverFullState::PresentationRequestReceived(_) => false,
            ProverFullState::PresentationPrepared(_) => true,
            ProverFullState::PresentationPreparationFailed(_) => true,
            ProverFullState::PresentationSent(_) => true,
            ProverFullState::Finished(_) => false,
        }
    }

    pub fn get_presentation_status(&self) -> u32 {
        match self.state {
            ProverFullState::Finished(ref state) => state.status.code(),
            _ => Status::Undefined.code(),
        }
    }

    pub fn get_presentation_request(&self) -> VcxResult<&RequestPresentation> {
        match self.state {
            ProverFullState::Initial(_) => Err(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation request is not available",
            )),
            ProverFullState::PresentationProposalSent(_) => Err(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation request is not available",
            )),
            ProverFullState::PresentationRequestReceived(ref state) => Ok(&state.presentation_request),
            ProverFullState::PresentationPrepared(ref state) => Ok(&state.presentation_request),
            ProverFullState::PresentationPreparationFailed(ref state) => Ok(&state.presentation_request),
            ProverFullState::PresentationSent(ref state) => Ok(&state.presentation_request),
            ProverFullState::Finished(ref state) => Ok(state.presentation_request.as_ref().ok_or(
                AriesVcxError::from_msg(AriesVcxErrorKind::NotReady, "Presentation request is not available"),
            )?),
        }
    }

    pub fn get_presentation_msg(&self) -> VcxResult<&Presentation> {
        match self.state {
            ProverFullState::Initial(_) => Err(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation is not created yet",
            )),
            ProverFullState::PresentationProposalSent(_) => Err(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation is not created yet",
            )),
            ProverFullState::PresentationRequestReceived(_) => Err(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation is not created yet",
            )),
            ProverFullState::PresentationPrepared(ref state) => Ok(&state.presentation),
            ProverFullState::PresentationPreparationFailed(_) => Err(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation is not created yet",
            )),
            ProverFullState::PresentationSent(ref state) => Ok(&state.presentation),
            ProverFullState::Finished(ref state) => Ok(state.presentation.as_ref().ok_or(AriesVcxError::from_msg(
                AriesVcxErrorKind::NotReady,
                "Presentation is not available in Finished state",
            ))?),
        }
    }
}

// #[cfg(test)]
// pub mod unit_tests {
//     use crate::common::test_utils::mock_profile;
//     use crate::test::source_id;
//     use crate::utils::devsetup::SetupMocks;
//     use messages::protocols::proof_presentation::presentation::test_utils::_presentation;
//     use messages::protocols::proof_presentation::presentation_proposal::test_utils::{
//         _presentation_preview, _presentation_proposal, _presentation_proposal_data,
//     };
//     use messages::protocols::proof_presentation::presentation_request::test_utils::_presentation_request;
//     use messages::protocols::proof_presentation::test_utils::{_ack, _problem_report};

//     use super::*;

//     pub fn _prover_sm_from_request() -> ProverSM {
//         ProverSM::from_request(_presentation_request(), source_id())
//     }

//     pub fn _send_message() -> Option<SendClosure> {
//         Some(Box::new(|_: A2AMessage| Box::pin(async { VcxResult::Ok(()) })))
//     }

//     pub fn _prover_sm() -> ProverSM {
//         ProverSM::new(source_id())
//     }

//     impl ProverSM {
//         async fn to_presentation_proposal_sent_state(mut self) -> ProverSM {
//             self = self
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationProposalSend(_presentation_proposal_data()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             self
//         }

//         async fn to_presentation_prepared_state(mut self) -> ProverSM {
//             self = self
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     None,
//                 )
//                 .await
//                 .unwrap();
//             self
//         }

//         async fn to_presentation_sent_state(mut self) -> ProverSM {
//             self = self
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             self = self
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             self
//         }

//         async fn to_finished_state(mut self) -> ProverSM {
//             self = self
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     None,
//                 )
//                 .await
//                 .unwrap();
//             self = self
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             self = self
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             self
//         }
//     }

//     fn _credentials() -> String {
//         json!({
//             "attrs":{
//             "attribute_0":{
//                 "credential":{
//                     "cred_info":{
//                         "attrs":{"name": "alice"},
//                         "cred_def_id": "V4SGRU86Z58d6TV7PBUe6f:3:CL:419:tag",
//                         "referent": "a1991de8-8317-43fd-98b3-63bac40b9e8b",
//                         "schema_id": "V4SGRU86Z58d6TV7PBUe6f:2:QcimrRShWQniqlHUtIDddYP0n:1.0"
//                         }
//                     }
//                 }
//             }
//         })
//         .to_string()
//     }

//     fn _self_attested() -> String {
//         json!({}).to_string()
//     }

//     mod build_messages {
//         use crate::protocols::common::build_problem_report_msg;
//         use messages::a2a::MessageId;

//         use crate::protocols::proof_presentation::prover::state_machine::build_presentation_msg;
//         use crate::utils::devsetup::{was_in_past, SetupMocks};

//         #[test]
//         fn test_prover_build_presentation_message() {
//             let _setup = SetupMocks::init();

//             let msg = build_presentation_msg("12345", "{}".into()).unwrap();

//             assert_eq!(msg.id, MessageId::default());
//             assert_eq!(msg.thread.thid, Some("12345".into()));
//             assert!(was_in_past(
//                 &msg.timing.unwrap().out_time.unwrap(),
//                 chrono::Duration::milliseconds(100)
//             )
//             .unwrap());
//         }

//         #[tokio::test]
//         async fn test_prover_build_problem_report() {
//             let _setup = SetupMocks::init();

//             let msg = build_problem_report_msg(Some("foobar".into()), "12345");

//             assert_eq!(msg.id, MessageId::default());
//             assert_eq!(msg.thread.unwrap().thid, Some("12345".into()));
//             assert_eq!(msg.comment, Some("foobar".into()));
//             assert!(was_in_past(
//                 &msg.timing.unwrap().out_time.unwrap(),
//                 chrono::Duration::milliseconds(100)
//             )
//             .unwrap());
//         }
//     }

//     mod new {
//         use super::*;

//         #[tokio::test]
//         async fn test_prover_new() {
//             let _setup = SetupMocks::init();

//             let prover_sm = _prover_sm();

//             assert_match!(ProverFullState::Initial(_), prover_sm.state);
//             assert_eq!(source_id(), prover_sm.source_id());
//         }

//         #[tokio::test]
//         async fn test_prover_from_request() {
//             let _setup = SetupMocks::init();

//             let prover_sm = _prover_sm_from_request();

//             assert_match!(ProverFullState::PresentationRequestReceived(_), prover_sm.state);
//             assert_eq!(source_id(), prover_sm.source_id());
//         }
//     }

//     mod step {
//         use crate::utils::constants::CREDS_FROM_PROOF_REQ;
//         use crate::utils::mockdata::mock_settings::MockBuilder;

//         use super::*;

//         #[tokio::test]
//         async fn test_prover_init() {
//             let _setup = SetupMocks::init();

//             let prover_sm = _prover_sm_from_request();
//             assert_match!(ProverFullState::PresentationRequestReceived(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_presentation_proposal_send_from_initial_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationProposalSend(_presentation_proposal_data()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::PresentationProposalSent(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_presentation_request_received_from_presentation_proposal_sent_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request().to_presentation_proposal_sent_state().await;
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationRequestReceived(_presentation_request()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::PresentationRequestReceived(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_presentation_reject_received_from_presentation_proposal_sent_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request().to_presentation_proposal_sent_state().await;
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationRejectReceived(_problem_report()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//             assert_eq!(
//                 Status::Declined(ProblemReport::default()).code(),
//                 prover_sm.get_presentation_status()
//             );
//         }

//         #[tokio::test]
//         async fn test_prover_handle_presentation_proposal_send_from_presentation_request_received_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationProposalSend(_presentation_proposal_data()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::PresentationProposalSent(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_set_presentation_from_presentation_request_received_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::SetPresentation(_presentation()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::PresentationPrepared(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_prepare_presentation_message_from_presentation_request_received_state_for_invalid_credentials(
//         ) {
//             let _setup = SetupMocks::init();
//             let _mock_builder = MockBuilder::init().set_mock_creds_retrieved_for_proof_request(CREDS_FROM_PROOF_REQ);

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation(("invalid".to_string(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::PresentationPreparationFailed(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_reject_presentation_request_message_from_presentation_request_received_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::RejectPresentationRequest(String::from("reject request")),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_propose_presentation_message_from_presentation_request_received_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::ProposePresentation(_presentation_preview()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_other_messages_from_presentation_request_received_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();

//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationRequestReceived(_), prover_sm.state);

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationRequestReceived(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_send_presentation_message_from_presentation_prepared_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::PresentationSent(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_other_messages_from_presentation_prepared_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request().to_presentation_prepared_state().await;

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationRejectReceived(_problem_report()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationPrepared(_), prover_sm.state);

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationPrepared(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_reject_presentation_request_message_from_presentation_prepared_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request().to_presentation_prepared_state().await;

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::RejectPresentationRequest(String::from("reject request")),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_propose_presentation_message_from_presentation_prepared_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request().to_presentation_prepared_state().await;
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::ProposePresentation(_presentation_preview()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_send_presentation_message_from_presentation_preparation_failed_state() {
//             let _setup = SetupMocks::init();
//             let _mock_builder = MockBuilder::init().set_mock_creds_retrieved_for_proof_request(CREDS_FROM_PROOF_REQ);

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation(("invalid".to_string(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationPreparationFailed(_), prover_sm.state);

//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//             assert_eq!(
//                 Status::Failed(ProblemReport::default()).code(),
//                 prover_sm.get_presentation_status()
//             );
//         }

//         #[tokio::test]
//         async fn test_prover_handle_other_messages_from_presentation_preparation_failed_state() {
//             let _setup = SetupMocks::init();
//             let _mock_builder = MockBuilder::init().set_mock_creds_retrieved_for_proof_request(CREDS_FROM_PROOF_REQ);

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation(("invalid".to_string(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationRejectReceived(_problem_report()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationPreparationFailed(_), prover_sm.state);

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationPreparationFailed(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_ack_message_from_presentation_sent_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//             assert_eq!(Status::Success.code(), prover_sm.get_presentation_status());
//         }

//         #[tokio::test]
//         async fn test_prover_handle_reject_presentation_request_message_from_presentation_sent_state() {
//             let _setup = SetupMocks::init();

//             let prover_sm = _prover_sm_from_request().to_presentation_sent_state().await;
//             let err = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::RejectPresentationRequest(String::from("reject")),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap_err();
//             assert_eq!(AriesVcxErrorKind::ActionNotSupported, err.kind());
//         }

//         #[tokio::test]
//         async fn test_prover_handle_presentation_reject_message_from_presentation_sent_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationRejectReceived(_problem_report()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//             assert_eq!(
//                 Status::Failed(ProblemReport::create()).code(),
//                 prover_sm.get_presentation_status()
//             );
//         }

//         #[tokio::test]
//         async fn test_prover_handle_other_messages_from_presentation_sent_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationSent(_), prover_sm.state);

//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::PresentationSent(_), prover_sm.state);
//         }

//         #[tokio::test]
//         async fn test_prover_handle_messages_from_finished_state() {
//             let _setup = SetupMocks::init();

//             let mut prover_sm = _prover_sm_from_request();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PreparePresentation((_credentials(), _self_attested())),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(&mock_profile(), ProverMessages::SendPresentation, _send_message())
//                 .await
//                 .unwrap();
//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationAckReceived(_ack()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::Finished(_), prover_sm.state);

//             prover_sm = prover_sm
//                 .step(
//                     &mock_profile(),
//                     ProverMessages::PresentationRejectReceived(_problem_report()),
//                     _send_message(),
//                 )
//                 .await
//                 .unwrap();
//             assert_match!(ProverFullState::Finished(_), prover_sm.state);
//         }
//     }

//     mod find_message_to_handle {
//         use super::*;

//         #[test]
//         fn test_prover_find_message_to_handle_from_intial_state() {
//             let _setup = SetupMocks::init();
//             let prover = _prover_sm();
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::Presentation(_presentation()),
//                     "key_3".to_string() => A2AMessage::PresentationRequest(_presentation_request()),
//                     "key_4".to_string() => A2AMessage::PresentationAck(_ack()),
//                     "key_5".to_string() => A2AMessage::CommonProblemReport(_problem_report())
//                 );

//                 assert!(prover.find_message_to_handle(messages).is_none());
//             }
//         }

//         #[test]
//         fn test_prover_find_message_to_handle_from_presentation_request_received_state() {
//             let _setup = SetupMocks::init();

//             let prover = _prover_sm_from_request();

//             // No messages
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::Presentation(_presentation()),
//                     "key_3".to_string() => A2AMessage::PresentationRequest(_presentation_request()),
//                     "key_4".to_string() => A2AMessage::PresentationAck(_ack()),
//                     "key_5".to_string() => A2AMessage::CommonProblemReport(_problem_report())
//                 );

//                 assert!(prover.find_message_to_handle(messages).is_none());
//             }
//         }

//         #[tokio::test]
//         async fn test_prover_find_message_to_handle_from_presentation_prepared_state() {
//             let _setup = SetupMocks::init();

//             let prover = _prover_sm_from_request().to_presentation_prepared_state().await;

//             // No messages
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::Presentation(_presentation()),
//                     "key_3".to_string() => A2AMessage::PresentationRequest(_presentation_request()),
//                     "key_4".to_string() => A2AMessage::PresentationAck(_ack()),
//                     "key_5".to_string() => A2AMessage::CommonProblemReport(_problem_report())
//                 );

//                 assert!(prover.find_message_to_handle(messages).is_none());
//             }
//         }

//         #[tokio::test]
//         async fn test_prover_find_message_to_handle_from_presentation_sent_state() {
//             let _setup = SetupMocks::init();

//             let prover = _prover_sm_from_request().to_presentation_sent_state().await;

//             // Ack
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::Presentation(_presentation()),
//                     "key_3".to_string() => A2AMessage::PresentationAck(_ack())
//                 );

//                 let (uid, message) = prover.find_message_to_handle(messages).unwrap();
//                 assert_eq!("key_3", uid);
//                 assert_match!(A2AMessage::PresentationAck(_), message);
//             }

//             // Problem Report
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::PresentationRequest(_presentation_request()),
//                     "key_3".to_string() => A2AMessage::CommonProblemReport(_problem_report())
//                 );

//                 let (uid, message) = prover.find_message_to_handle(messages).unwrap();
//                 assert_eq!("key_3", uid);
//                 assert_match!(A2AMessage::CommonProblemReport(_), message);
//             }

//             // No messages for different Thread ID
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal().set_thread_id("")),
//                     "key_2".to_string() => A2AMessage::Presentation(_presentation().set_thread_id("")),
//                     "key_3".to_string() => A2AMessage::PresentationAck(_ack().set_thread_id("")),
//                     "key_4".to_string() => A2AMessage::CommonProblemReport(_problem_report().set_thread_id(""))
//                 );

//                 assert!(prover.find_message_to_handle(messages).is_none());
//             }

//             // No messages
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::PresentationRequest(_presentation_request())
//                 );

//                 assert!(prover.find_message_to_handle(messages).is_none());
//             }
//         }

//         #[tokio::test]
//         async fn test_prover_find_message_to_handle_from_finished_state() {
//             let _setup = SetupMocks::init();

//             let prover = _prover_sm_from_request().to_finished_state().await;

//             // No messages
//             {
//                 let messages = map!(
//                     "key_1".to_string() => A2AMessage::PresentationProposal(_presentation_proposal()),
//                     "key_2".to_string() => A2AMessage::Presentation(_presentation()),
//                     "key_3".to_string() => A2AMessage::PresentationRequest(_presentation_request()),
//                     "key_4".to_string() => A2AMessage::PresentationAck(_ack()),
//                     "key_5".to_string() => A2AMessage::CommonProblemReport(_problem_report())
//                 );

//                 assert!(prover.find_message_to_handle(messages).is_none());
//             }
//         }
//     }

//     mod get_state {
//         use super::*;

//         #[tokio::test]
//         async fn test_get_state() {
//             let _setup = SetupMocks::init();

//             assert_eq!(
//                 ProverState::PresentationRequestReceived,
//                 _prover_sm_from_request().get_state()
//             );
//             assert_eq!(
//                 ProverState::PresentationPrepared,
//                 _prover_sm_from_request()
//                     .to_presentation_prepared_state()
//                     .await
//                     .get_state()
//             );
//             assert_eq!(
//                 ProverState::PresentationSent,
//                 _prover_sm_from_request().to_presentation_sent_state().await.get_state()
//             );
//             assert_eq!(
//                 ProverState::Finished,
//                 _prover_sm_from_request().to_finished_state().await.get_state()
//             );
//         }
//     }
// }
