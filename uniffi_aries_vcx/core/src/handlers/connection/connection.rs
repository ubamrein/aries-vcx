use diddoc_legacy::aries::diddoc::AriesDidDoc;
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
    pub fn unpack_msg(&self, profile: Arc<ProfileHolder>, msg: String) -> VcxUniFFIResult<TypeMessage> {
        let _guard = self.handler.lock()?;
        let w = profile.inner.inject_wallet();
        let decrypted_package = block_on(w.unpack_message(msg.as_bytes()))?;
        let decrypted_package =
            std::str::from_utf8(&decrypted_package).map_err(|_| VcxUniFFIError::SerializationError {
                error_msg: "Wrong encoding".to_string(),
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
        Ok(TypeMessage { ty, content })
    }
    pub fn get_state(&self) -> VcxUniFFIResult<ConnectionState> {
        let handler = self.handler.lock()?;
        Ok(ConnectionState::from(handler.state()))
    }

    pub fn pairwise_info(&self) -> VcxUniFFIResult<PairwiseInfo> {
        let handler = self.handler.lock()?;
        Ok(handler.pairwise_info().clone())
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
        block_on(async {
            let new_conn = connection
                .handle_request(
                    &profile.inner.inject_wallet(),
                    request,
                    url,
                    routing_keys,
                    &native_client,
                )
                .await?;

            *handler = VcxGenericConnection::from(new_conn);

            Ok(())
        })
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
    use std::{sync::Arc, time::Duration};

    use aries_vcx::{
        aries_vcx_core::indy::wallet::WalletConfigBuilder,
        messages::{
            msg_fields::protocols::basic_message::{BasicMessage, BasicMessageContent, BasicMessageDecorators},
            msg_parts::MsgParts,
            AriesMessage,
        },
    };
    use chrono::Utc;
    use serde_json::Value;

    use crate::{
        core::{
            http_client::{self, NativeClient},
            profile::{new_indy_profile, ProfileHolder},
        },
        handlers::connection::connection::create_invitee,
    };

    use super::{create_inviter, Connection};
    static INVITER_URL_INBOUND: &str = "https://did-relay.ubique.ch/msg/fancy-pancy-inviter";
    static INVITER_URL_MAILBOX: &str = "https://did-relay.ubique.ch/get_msg/fancy-pancy-inviter";
    static INVITEE_URL_INBOUND: &str = "https://did-relay.ubique.ch/msg/fancy-pancy-invitee";
    static INVITEE_URL_MAILBOX: &str = "https://did-relay.ubique.ch/get_msg/fancy-pancy-invitee";

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
        let profile = new_indy_profile(wallet_config, Arc::new(native_client)).unwrap();
        let inviter = create_inviter(profile.clone()).unwrap();
        let invitation = inviter.create_invitation(INVITER_URL_INBOUND.to_string()).unwrap();
        println!("{invitation}");

        let invitee_profile = new_indy_profile(invitee_wallet_config, Arc::new(invitee_native_client)).unwrap();

        let invitee = create_invitee(invitee_profile.clone()).unwrap();
        invitee.accept_invitation(invitee_profile.clone(), invitation).unwrap();
        invitee
            .send_request(invitee_profile.clone(), INVITEE_URL_INBOUND.to_string(), vec![])
            .unwrap();
        // std::thread::sleep(Duration::from_secs(3));
        let msg = ureq::get(INVITER_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let mut msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        assert_eq!(msgs.len(), 1);
        let connection_request = msgs.pop().unwrap();
        let connection_request = serde_json::to_string(&connection_request).unwrap();
        let connection_request = inviter.unpack_msg(profile.clone(), connection_request).unwrap();
        println!("{}", connection_request.content);
        inviter
            .handle_request(
                profile.clone(),
                connection_request.content,
                INVITER_URL_INBOUND.to_string(),
                vec![],
            )
            .unwrap();
        inviter.send_response(profile.clone()).unwrap();

        // std::thread::sleep(Duration::from_secs(3));
        let msg = ureq::get(INVITEE_URL_MAILBOX).call().unwrap().into_string().unwrap();
        let mut msgs: Vec<Value> = serde_json::from_str(&msg).unwrap();
        assert_eq!(msgs.len(), 1);
        let connection_response = msgs.pop().unwrap();
        let connection_response = serde_json::to_string(&connection_response).unwrap();
        let connection_response = invitee
            .unpack_msg(invitee_profile.clone(), connection_response)
            .unwrap();
        invitee
            .handle_response(invitee_profile.clone(), connection_response.content)
            .unwrap();
        ((profile, inviter), (invitee_profile, invitee))
    }
    #[test]
    fn test_invitation() {
        // clean pipe
        let _ = ureq::get(INVITEE_URL_MAILBOX).call();
        let _ = ureq::get(INVITER_URL_MAILBOX).call();

        let (inviter, invitee) = establish_connection();

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
}
