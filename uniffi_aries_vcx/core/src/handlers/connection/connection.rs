use base64::Engine;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use aries_vcx::{
    errors::error::{AriesVcxError, AriesVcxErrorKind, VcxResult},
    handlers::util::AnyInvitation,
    messages::AriesMessage,
    protocols::connection::pairwise_info::PairwiseInfo,
    protocols::connection::GenericConnection as VcxGenericConnection,
    protocols::{connection::Connection as VcxConnection, SendClosure},
};
use url::Url;

use crate::{
    core::profile::ProfileHolder,
    errors::error::{VcxUniFFIError, VcxUniFFIResult},
    handlers::TypeMessage,
    runtime::block_on,
};

use super::ConnectionState;
pub struct Connection {
    handler: Mutex<VcxGenericConnection>,
}

// seperate function since uniffi can't handle constructors with results
pub fn create_inviter(profile: Arc<ProfileHolder>) -> VcxUniFFIResult<Arc<Connection>> {
    block_on(async {
        let pairwise_info = PairwiseInfo::create(&profile.inner.inject_wallet()).await?;
        let connection = VcxConnection::new_inviter(String::new(), pairwise_info);
        let handler = Mutex::new(VcxGenericConnection::from(connection));
        Ok(Arc::new(Connection { handler }))
    })
}

pub fn from_str(json: String) -> VcxUniFFIResult<Arc<Connection>> {
    let c: VcxGenericConnection = serde_json::from_str(&json)?;
    let handler = Mutex::new(c);
    Ok(Arc::new(Connection { handler }))
}

// seperate function since uniffi can't handle constructors with results
pub fn create_invitee(profile: Arc<ProfileHolder>) -> VcxUniFFIResult<Arc<Connection>> {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Trace));
    block_on(async {
        let pairwise_info = PairwiseInfo::create(&profile.inner.inject_wallet()).await?;
        let connection = VcxConnection::new_invitee(String::new(), pairwise_info);
        let handler = Mutex::new(VcxGenericConnection::from(connection));

        Ok(Arc::new(Connection { handler }))
    })
}

impl Connection {
    pub fn serialize_to_string(&self) -> VcxUniFFIResult<String> {
        let guard = self.handler.lock()?;
        let c: &VcxGenericConnection = &guard;
        Ok(serde_json::to_string(c)?)
    }
    pub fn unpack_msg(&self, profile: Arc<ProfileHolder>, msg: String) -> VcxUniFFIResult<TypeMessage> {
        let _guard = self.handler.lock()?;
        let w = profile.inner.inject_wallet();

        let enc_msg: Value = serde_json::from_str(&msg)?;
        let Some(protected) = enc_msg.get("protected").map(|a| a.as_str()).flatten() else {
            return Err(VcxUniFFIError::SerializationError {
                error_msg: "Nothing to unpack".to_string(),
            });
        };
        let protected_string =
            base64::prelude::BASE64_STANDARD
                .decode(protected)
                .map_err(|_| VcxUniFFIError::SerializationError {
                    error_msg: "Wrong encoding".to_string(),
                })?;
        // let protected_obj: Value = serde_json::from_slice(&protected_string)?;

        let decrypted_package = block_on(w.unpack_message(msg.as_bytes()))?;
        let decrypted_package =
            std::str::from_utf8(&decrypted_package).map_err(|_| VcxUniFFIError::SerializationError {
                error_msg: "Wrong encoding".to_string(),
            })?;
        let decrypted_package: Value = serde_json::from_str(decrypted_package)?;
        println!("{decrypted_package}");
        let msg = decrypted_package
            .get("message")
            .ok_or_else(|| VcxUniFFIError::SerializationError {
                error_msg: "Message not found".to_string(),
            })?
            .as_str()
            .ok_or_else(|| VcxUniFFIError::SerializationError {
                error_msg: "Message not a string".to_string(),
            })?;

        let id = decrypted_package
            .get("recipient_verkey")
            .and_then(|a| a.as_str())
            .unwrap();

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
        Ok(TypeMessage {
            kid: id.to_string(),
            ty,
            content,
        })
    }
    pub fn get_state(&self) -> VcxUniFFIResult<ConnectionState> {
        let handler = self.handler.lock()?;
        Ok(ConnectionState::from(handler.state()))
    }

    pub fn pairwise_info(&self) -> VcxUniFFIResult<PairwiseInfo> {
        let handler = self.handler.lock()?;
        Ok(handler.pairwise_info().clone())
    }
    pub fn key_id(&self, profile: Arc<ProfileHolder>) -> VcxUniFFIResult<String> {
        let handler = self.handler.lock()?;

        Ok(handler.pairwise_info().pw_vk.to_string())
    }
    pub fn create_invitation(&self, service_endpoint: String) -> VcxUniFFIResult<String> {
        let mut handler = self.handler.lock()?;
        let connection = VcxConnection::try_from(handler.clone())?;
        let url: Url = service_endpoint.parse().map_err(|_| VcxUniFFIError::InternalError {
            error_msg: "service_endpoint is not an url".to_string(),
        })?;
        let invite = connection.create_invitation(vec![], url);
        let AnyInvitation::Con(invitation) = invite.get_invitation().to_owned() else {
            return Err(VcxUniFFIError::InternalError { error_msg: "Unexpected invite".to_string() });
        };
        *handler = invite.into();
        let invitation = serde_json::to_string(&invitation)?;
        Ok(invitation)
    }

    // NOTE : using string here out of laziness. We could have type this,
    // but UniFFI does not support structs with unnamed fields. So we'd have to
    // wrap these types
    // here invitation -> aries_vcx::Invitation
    pub fn accept_invitation(&self, profile: Arc<ProfileHolder>, invitation: String) -> VcxUniFFIResult<()> {
        let mut handler = self.handler.lock()?;
        let invitation = serde_json::from_str(&invitation)?;

        let connection = VcxConnection::try_from(handler.clone())?;

        block_on(async {
            let new_conn = connection.accept_invitation(&profile.inner, invitation).await?;
            *handler = VcxGenericConnection::from(new_conn);
            Ok(())
        })
    }
    pub fn get_their_did_doc(&self) -> VcxUniFFIResult<String> {
        let handler = self.handler.lock()?;
        let doc = handler.their_did_doc().ok_or_else(|| VcxUniFFIError::InternalError {
            error_msg: "no did doc".to_string(),
        })?;
        Ok(serde_json::to_string(doc).unwrap())
    }

    // NOTE : using string here out of laziness. We could have type this,
    // but UniFFI does not support structs with unnamed fields. So we'd have to
    // wrap these types
    // here request -> aries_vcx::Request
    pub fn handle_request(
        &self,
        profile: Arc<ProfileHolder>,
        request: String,
        service_endpoint: String,
        routing_keys: Vec<String>,
    ) -> VcxUniFFIResult<()> {
        let mut handler = self.handler.lock()?;
        let request = serde_json::from_str(&request)?;

        let connection = VcxConnection::try_from(handler.clone())?;
        let url = Url::parse(&service_endpoint)
            .map_err(|err| AriesVcxError::from_msg(AriesVcxErrorKind::InvalidUrl, err.to_string()))?;
        let native_client = profile.transport.clone();
        *handler = block_on(async {
            let new_conn = connection
                .handle_request(
                    &profile.inner.inject_wallet(),
                    request,
                    url,
                    routing_keys,
                    &native_client,
                )
                .await?;
            Ok::<_, VcxUniFFIError>(VcxGenericConnection::from(new_conn))
        })?;
        Ok(())
    }

    // NOTE : using string here out of laziness. We could have type this,
    // but UniFFI does not support structs with unnamed fields. So we'd have to
    // wrap these types
    // here request -> aries_vcx::Request
    pub fn handle_response(&self, profile: Arc<ProfileHolder>, response: String) -> VcxUniFFIResult<()> {
        let mut handler = self.handler.lock()?;
        let response = serde_json::from_str(&response)?;

        let connection = VcxConnection::try_from(handler.clone())?;
        let native_client = profile.transport.clone();
        block_on(async {
            let new_conn = connection
                .handle_response(&profile.inner.inject_wallet(), response, &native_client)
                .await?;
            *handler = VcxGenericConnection::from(new_conn);

            Ok(())
        })
    }

    pub fn send_request(
        &self,
        profile: Arc<ProfileHolder>,
        service_endpoint: String,
        routing_keys: Vec<String>,
    ) -> VcxUniFFIResult<()> {
        let mut handler = self.handler.lock()?;

        let connection = VcxConnection::try_from(handler.clone())?;
        let url = Url::parse(&service_endpoint)
            .map_err(|err| AriesVcxError::from_msg(AriesVcxErrorKind::InvalidUrl, err.to_string()))?;
        let native_client = profile.transport.clone();
        block_on(async {
            let new_conn = connection
                .send_request(&profile.inner.inject_wallet(), url, routing_keys, &native_client)
                .await?;
            *handler = VcxGenericConnection::from(new_conn);

            Ok(())
        })
    }

    pub fn send_response(&self, profile: Arc<ProfileHolder>) -> VcxUniFFIResult<()> {
        let mut handler = self.handler.lock()?;

        let connection = VcxConnection::try_from(handler.clone())?;
        let native_client = profile.transport.clone();
        block_on(async {
            let new_conn = connection
                .send_response(&profile.inner.inject_wallet(), &native_client)
                .await?;

            *handler = VcxGenericConnection::from(new_conn);

            Ok(())
        })
    }

    pub fn send_message(&self, profile: Arc<ProfileHolder>) -> SendClosure {
        let handler = self.handler.lock().unwrap();
        let connection = handler.clone();
        let native_client = profile.transport.clone();
        Box::new(move |m| {
            Box::pin(async move {
                connection
                    .send_message(&profile.inner.inject_wallet(), &m, &native_client)
                    .await?;
                VcxResult::Ok(())
            })
        })
    }
    pub fn send_custom_message(&self, profile: Arc<ProfileHolder>, msg: AriesMessage) -> VcxUniFFIResult<()> {
        let handler = self.handler.lock().unwrap();
        let connection = handler.clone();
        let native_client = profile.transport.clone();
        block_on(async move {
            connection
                .send_message(&profile.inner.inject_wallet(), &msg, &native_client)
                .await
        })?;
        Ok(())
    }

    pub fn send_ack(&self, profile: Arc<ProfileHolder>) -> VcxUniFFIResult<()> {
        let mut handler = self.handler.lock()?;

        let connection = VcxConnection::try_from(handler.clone())?;
        let native_client = profile.transport.clone();
        block_on(async {
            let new_conn = connection
                .send_ack(&profile.inner.inject_wallet(), &native_client)
                .await?;
            *handler = VcxGenericConnection::from(new_conn);

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread, time::Duration};

    use aries_vcx::{
        aries_vcx_core::indy::wallet::WalletConfigBuilder,
        common::proofs::{proof_request::ProofRequestData, proof_request_internal::AttrInfo},
        handlers::proof_presentation::types::{RetrievedCredentials, SelectedCredentials},
        messages::{
            msg_fields::protocols::basic_message::{BasicMessageContent, BasicMessageDecorators},
            msg_parts::MsgParts,
            AriesMessage,
        },
    };
    use serde_json::Value;

    use crate::{
        core::{
            http_client::{self, NativeClient},
            profile::{new_indy_profile, ProfileHolder},
        },
        handlers::{
            connection::connection::create_invitee,
            issuance::issuance::{create_vc_receiver, get_indy_credential},
            proof::{proof::Proof, verify::Verify},
            TypeMessage,
        },
        runtime::block_on,
    };

    use super::{create_inviter, Connection};
    static INVITER_URL_INBOUND: &str = "https://did-relay.ubique.ch/msg/fancy-pancy-inviter";
    static INVITER_URL_MAILBOX: &str = "https://did-relay.ubique.ch/get_msg/fancy-pancy-inviter";
    static INVITEE_URL_INBOUND: &str = "https://did-relay.ubique.ch/msg/fancy-pancy-invitee";
    static INVITEE_URL_MAILBOX: &str = "https://did-relay.ubique.ch/get_msg/fancy-pancy-invitee";

    static LEDGER_BASE_URL: &str = "https://tg4u-ws-dev.ubique.ch/v1";

    fn establish_connection() -> (
        (Arc<ProfileHolder>, Arc<Connection>),
        (Arc<ProfileHolder>, Arc<Connection>),
    ) {
        let native_client = NativeClient::new(Box::new(http_client::HttpClient));
        let wallet_config = WalletConfigBuilder::default()
            .wallet_name("test")
            .wallet_key("1234")
            .wallet_key_derivation("ARGON2I_MOD")
            .build()
            .unwrap();
        let invitee_native_client = NativeClient::new(Box::new(http_client::HttpClient));
        let invitee_wallet_config = WalletConfigBuilder::default()
            .wallet_name("test-invitee")
            .wallet_key("1234")
            .wallet_key_derivation("ARGON2I_MOD")
            .build()
            .unwrap();
        let profile = new_indy_profile(wallet_config, Arc::new(native_client), LEDGER_BASE_URL.to_string()).unwrap();
        let inviter = create_inviter(profile.clone()).unwrap();
        let invitation = inviter.create_invitation(INVITER_URL_INBOUND.to_string()).unwrap();
        println!("{invitation}");

        let invitee_profile = new_indy_profile(
            invitee_wallet_config,
            Arc::new(invitee_native_client),
            LEDGER_BASE_URL.to_string(),
        )
        .unwrap();

        let invitee = create_invitee(invitee_profile.clone()).unwrap();
        invitee.accept_invitation(invitee_profile.clone(), invitation).unwrap();
        invitee
            .send_request(invitee_profile.clone(), INVITEE_URL_INBOUND.to_string(), vec![])
            .unwrap();
        println!("---> Invitee-VK: {}", invitee.pairwise_info().unwrap().pw_vk);

        let msg = ureq::get(INVITER_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let mut msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        assert_eq!(msgs.len(), 1);
        // println!("{}", serde_json::to_string(&msgs).unwrap());
        let connection_request = msgs.pop().unwrap();
        // println!("{}", serde_json::to_string(&msgs).unwrap());
        let connection_request = serde_json::to_string(&connection_request).unwrap();
        let connection_request = inviter.unpack_msg(profile.clone(), connection_request).unwrap();
        // println!("{}", connection_request.content);
        inviter
            .handle_request(
                profile.clone(),
                connection_request.content,
                INVITER_URL_INBOUND.to_string(),
                vec![],
            )
            .unwrap();
        inviter.send_response(profile.clone()).unwrap();
        println!("---> Inviter-VK: {}", inviter.pairwise_info().unwrap().pw_vk);

        // std::thread::sleep(Duration::from_secs(3));
        let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let mut msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        assert_eq!(msgs.len(), 1);
        let connection_response = msgs.pop().unwrap();

        let connection_response = serde_json::to_string(&connection_response).unwrap();
        let connection_response = invitee
            .unpack_msg(invitee_profile.clone(), connection_response)
            .unwrap();

        println!("--> Msg-kid: {}", connection_response.kid);
        println!("--> Msg-Type: {}", connection_response.ty);
        println!("--> Msg-Content: {}", connection_response.content);
        assert_eq!(connection_response.kid, invitee.pairwise_info().unwrap().pw_vk);
        invitee
            .handle_response(invitee_profile.clone(), connection_response.content)
            .unwrap();
        ((profile, inviter), (invitee_profile, invitee))
    }

    fn establish_connection_with_invite(invitation: &str) -> (Arc<ProfileHolder>, Arc<Connection>, Vec<TypeMessage>) {
        let invitee_native_client = NativeClient::new(Box::new(http_client::HttpClient));
        let invitee_wallet_config = WalletConfigBuilder::default()
            .wallet_name("test-invitee")
            .wallet_key("1234")
            .wallet_key_derivation("ARGON2I_MOD")
            .build()
            .unwrap();
        let invitee_profile = new_indy_profile(
            invitee_wallet_config,
            Arc::new(invitee_native_client),
            LEDGER_BASE_URL.to_string(),
        )
        .unwrap();
        let invitee = create_invitee(invitee_profile.clone()).unwrap();

        invitee
            .accept_invitation(invitee_profile.clone(), invitation.to_string())
            .unwrap();
        invitee
            .send_request(invitee_profile.clone(), INVITEE_URL_INBOUND.to_string(), vec![])
            .unwrap();
        println!("give it some time...");
        std::thread::sleep(Duration::from_secs(3));
        let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        let invitee_profile_clone = invitee_profile.clone();
        let invitee_clone = invitee.clone();
        let decrypted_vals = msgs
            .into_iter()
            .map(|m| {
                invitee_clone
                    .unpack_msg(invitee_profile_clone.clone(), m.to_string())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let connection_response = decrypted_vals.iter().find(|m| m.ty.contains("response")).unwrap();

        invitee
            .handle_response(invitee_profile.clone(), connection_response.content.to_string())
            .unwrap();
        let _ = invitee.send_ack(invitee_profile.clone());
        (invitee_profile, invitee, decrypted_vals)
    }
    #[test]
    fn test_invitation() {
        // clean pipe
        let _ = ureq::get(INVITEE_URL_MAILBOX).call();
        let _ = ureq::get(INVITER_URL_MAILBOX).call();

        let (inviter, invitee) = establish_connection();
        println!("-------> Connection established <---------");
        let content = BasicMessageContent::new("Hallo".to_string(), chrono::Utc::now());
        let msg = AriesMessage::BasicMessage(MsgParts::with_decorators(
            "test-invitee".to_string(),
            content,
            BasicMessageDecorators::default(),
        ));
        invitee.1.send_custom_message(invitee.0.clone(), msg).unwrap();
        // std::thread::sleep(Duration::from_secs(3));
        let msg = ureq::get(INVITER_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        assert_eq!(msgs.len(), 1);
        let msg = serde_json::to_string(&msgs[0]).unwrap();
        println!("Encrypted Message: {msg}");
        let msg = inviter.1.unpack_msg(inviter.0.clone(), msg).unwrap();
        let msg_value: Value = serde_json::from_str(&msg.content).unwrap();
        assert_eq!(
            Some(Value::String("Hallo".to_string())),
            msg_value.get("content").cloned()
        );

        let content = BasicMessageContent::new("Welt".to_string(), chrono::Utc::now());
        let msg = AriesMessage::BasicMessage(MsgParts::with_decorators(
            "test-inviter".to_string(),
            content,
            BasicMessageDecorators::default(),
        ));
        inviter.1.send_custom_message(inviter.0.clone(), msg).unwrap();
        let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        assert_eq!(msgs.len(), 1);
        let msg = serde_json::to_string(&msgs[0]).unwrap();
        let msg = invitee.1.unpack_msg(invitee.0.clone(), msg).unwrap();
        let msg_value: Value = serde_json::from_str(&msg.content).unwrap();
        assert_eq!(
            Some(Value::String("Welt".to_string())),
            msg_value.get("content").cloned()
        );
    }

    static NAME_ATTR: &str = r#"
{
    "name": "name",
    "restrictions": [
        {
            "cred_def_id": "2xS9NY4w46MUCXEtAWT7Gf:3:CL:47:social-id"
        },
        {
            "cred_def_id": "BqG7nxsaCGrbjsTJ8tcmEf:3:CL:166:1.0"
        }
    ]
}"#;

    #[test]
    fn test_remote_verifier() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .try_init();
        let _ = ureq::get(INVITEE_URL_MAILBOX).call();
        let _ = ureq::get(INVITER_URL_MAILBOX).call();

        {
            // use adnovum api to get creds
            let adnovum_invitee = establish_connection_with_invite(
                r#"{"@type": "did:sov:BzCbsNYhMrjHiqZDTUASHg;spec/connections/1.0/invitation", "@id": "16af8efb-9919-4a9d-a290-53aa1f679ac2", "recipientKeys": ["BXUBBQkYqvicBYugGQqto8tPZFRAHMZUjCmMPZoHaDun"], "serviceEndpoint": "https://ssi-start.adnovum.com/didcomm", "label": "SSI Self-Service Portal"}"#,
            );
            println!("Connection established, try getting credentials");
            let receiver = create_vc_receiver("test".to_string(), adnovum_invitee.1.clone()).unwrap();
            let mut got_offer = false;

            for m in &adnovum_invitee.2 {
                if m.ty.contains("offer-credential") {
                    receiver.receive_offer(m.content.to_string()).unwrap();
                    receiver.send_request(adnovum_invitee.0.clone()).unwrap();
                    got_offer = true;
                    println!("go offer");
                    break;
                }
            }

            while !got_offer {
                let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
                let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
                for m in msgs {
                    let m = adnovum_invitee
                        .1
                        .unpack_msg(adnovum_invitee.0.clone(), m.to_string())
                        .unwrap();
                    println!("{}", m.ty);
                    if m.ty.contains("offer-credential") {
                        receiver.receive_offer(m.content).unwrap();
                        receiver.send_request(adnovum_invitee.0.clone()).unwrap();
                        got_offer = true;
                        println!("go offer");
                        break;
                    }
                }
            }
            println!("Credential request sent waiting for response");
            let mut got_creds = false;
            while !got_creds {
                let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
                let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();

                for m in msgs {
                    let m = adnovum_invitee
                        .1
                        .unpack_msg(adnovum_invitee.0.clone(), m.to_string())
                        .unwrap();
                    println!("{}", m.ty);
                    if m.ty.contains("1.0/issue-credential") {
                        receiver
                            .process_credential(adnovum_invitee.0.clone(), m.content)
                            .unwrap();

                        got_creds = true;
                        let entry = receiver.get_credential().unwrap();
                        println!("{}", entry.credential_id);
                        println!("{}", entry.credential);
                        println!(
                            "-----!!!>{}",
                            get_indy_credential(adnovum_invitee.0.clone(), entry.credential_id.clone()).unwrap()
                        );
                        break;
                    }
                }
            }
            println!("we received the credentials");
            drop(receiver);
            drop(adnovum_invitee);
        }
        let remote_verifier = establish_connection_with_invite(
            r#"{"@type": "did:sov:BzCbsNYhMrjHiqZDTUASHg;spec/connections/1.0/invitation", "@id": "c5e09173-dfc9-49ed-a638-9ffbf6af3411", "label": "Aries Cloud Agent", "recipientKeys": ["FWVE2pNXwVgr1J6cwBhvue9jnk7jLH1uZxJXoAnGpxE7"], "serviceEndpoint": "https://tg4u-acapy-issuer-dev.ubique.ch"}"#,
        );

        let _ = ureq::get(INVITEE_URL_MAILBOX).call();
        thread::sleep(Duration::from_secs(5));
        println!("Connection established getting ready for proof");
        thread::sleep(Duration::from_secs(5));

        let mut msgs: Vec<Value> = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_json().unwrap();
        assert_eq!(msgs.len(), 1);
        let msg = msgs.pop().unwrap();

        let msg = remote_verifier
            .1
            .unpack_msg(remote_verifier.0.clone(), msg.to_string())
            .unwrap();
        println!("{}", msg.content);
        let proofer = Proof::create_from_request("test".to_string(), msg.content).unwrap();
        let creds: RetrievedCredentials =
            serde_json::from_str(&proofer.select_credentials(remote_verifier.0.clone()).unwrap()).unwrap();
        println!("{:?}", creds.credentials_by_referent);
        let first = &(&creds.credentials_by_referent["attribute_0"])[0];

        let mut sc = SelectedCredentials::default();
        sc.select_credential_for_referent_from_retrieved("attribute_0".to_string(), first.to_owned(), None);
        println!("try sending proof");
        proofer
            .send_presentation(
                remote_verifier.0.clone(),
                remote_verifier.1.clone(),
                serde_json::to_string(&sc).unwrap(),
            )
            .unwrap();
    }

    #[test]
    fn test_proof_api() {
        {
            let _ = env_logger::builder()
                .filter_level(log::LevelFilter::Info)
                .is_test(true)
                .try_init();
            let _ = ureq::get(INVITEE_URL_MAILBOX).call();
            let _ = ureq::get(INVITER_URL_MAILBOX).call();

            // use adnovum api to get creds
            let adnovum_invitee = establish_connection_with_invite(
                r#"{"@type": "did:sov:BzCbsNYhMrjHiqZDTUASHg;spec/connections/1.0/invitation", "@id": "bee831f8-8d20-4dd2-bde3-c3dd572983ac", "recipientKeys": ["6SuigWycBZL8PQZEFxR5o8BQeuVEFsYe9NhqsTja2jb3"], "serviceEndpoint": "https://ssi-start.adnovum.com/didcomm", "label": "SSI Self-Service Portal"}"#,
            );
            println!("Connection established, try getting credentials");
            let receiver = create_vc_receiver("test".to_string(), adnovum_invitee.1.clone()).unwrap();
            let mut got_offer = false;

            for m in &adnovum_invitee.2 {
                if m.ty.contains("offer-credential") {
                    receiver.receive_offer(m.content.to_string()).unwrap();
                    receiver.send_request(adnovum_invitee.0.clone()).unwrap();
                    got_offer = true;
                    println!("go offer");
                    break;
                }
            }

            while !got_offer {
                let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
                let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
                for m in msgs {
                    let m = adnovum_invitee
                        .1
                        .unpack_msg(adnovum_invitee.0.clone(), m.to_string())
                        .unwrap();
                    println!("{}", m.ty);
                    if m.ty.contains("offer-credential") {
                        receiver.receive_offer(m.content).unwrap();
                        receiver.send_request(adnovum_invitee.0.clone()).unwrap();
                        got_offer = true;
                        println!("go offer");
                        break;
                    }
                }
            }
            println!("Credential request sent waiting for response");
            let mut got_creds = false;
            while !got_creds {
                let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
                let msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();

                for m in msgs {
                    let m = adnovum_invitee
                        .1
                        .unpack_msg(adnovum_invitee.0.clone(), m.to_string())
                        .unwrap();
                    println!("{}", m.ty);
                    if m.ty.contains("1.0/issue-credential") {
                        receiver
                            .process_credential(adnovum_invitee.0.clone(), m.content)
                            .unwrap();

                        got_creds = true;
                        let entry = receiver.get_credential().unwrap();
                        println!("{}", entry.credential_id);
                        println!("{}", entry.credential);
                        println!(
                            "-----!!!>{}",
                            get_indy_credential(adnovum_invitee.0.clone(), entry.credential_id.clone()).unwrap()
                        );
                        break;
                    }
                }
            }
            println!("we received the credentials");
            drop(receiver);
            drop(adnovum_invitee);
        }
        // establish_connection between two parties
        let (inviter, invitee) = establish_connection();
        // start proof
        let _ = ureq::get(INVITEE_URL_MAILBOX).call();
        let _ = ureq::get(INVITER_URL_MAILBOX).call();

        let the_profile = inviter.0.clone();
        let proof_request = block_on(async move {
            let attr_info: AttrInfo = serde_json::from_str(NAME_ATTR).unwrap();
            ProofRequestData::create(&the_profile.inner, "Check stuff")
                .await
                .unwrap()
                .set_requested_attributes_as_vec(vec![attr_info])
                .unwrap()
        });
        let proof_request = serde_json::to_string(&proof_request).unwrap();
        println!("{proof_request}");
        let verifier = Verify::create_from_request("test".to_string(), proof_request).unwrap();
        verifier.send_request(inviter.0.clone(), inviter.1.clone()).unwrap();

        let mut msgs: Vec<Value> = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_json().unwrap();
        assert_eq!(msgs.len(), 1);
        let msg = msgs.pop().unwrap();

        let msg = invitee.1.unpack_msg(invitee.0.clone(), msg.to_string()).unwrap();
        println!("{}", msg.content);
        let proofer = Proof::create_from_request("test".to_string(), msg.content).unwrap();
        let creds: RetrievedCredentials =
            serde_json::from_str(&proofer.select_credentials(invitee.0.clone()).unwrap()).unwrap();
        println!("{:?}", creds.credentials_by_referent);
        let first = &(&creds.credentials_by_referent["attribute_0"])[0];

        let mut sc = SelectedCredentials::default();
        sc.select_credential_for_referent_from_retrieved("attribute_0".to_string(), first.to_owned(), None);
        println!("try sending proof");
        proofer
            .send_presentation(
                invitee.0.clone(),
                invitee.1.clone(),
                serde_json::to_string(&sc).unwrap(),
            )
            .unwrap();

        let mut msgs: Vec<Value> = ureq::get(INVITER_URL_MAILBOX).call().unwrap().into_json().unwrap();
        assert_eq!(msgs.len(), 1);
        let msg = msgs.pop().unwrap();
        let msg = inviter.1.unpack_msg(inviter.0.clone(), msg.to_string()).unwrap();
        println!("{}", msg.content);
        let result = verifier
            .verify(inviter.0.clone(), inviter.1.clone(), msg.content)
            .unwrap();
        assert!(result);

        let revealed = verifier.get_revealed_attr().unwrap();
        for r in revealed {
            println!("{}: {}", r.name, r.value)
        }
    }

    #[test]
    fn test_deserialize_schema() {
        let s = include_str!("../../../schema.json");
        let schema: vdrtools::Schema = serde_json::from_str(s).unwrap();
        println!("{:?}", schema);
    }
}
