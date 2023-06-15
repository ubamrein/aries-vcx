use diddoc_legacy::aries::diddoc::AriesDidDoc;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use aries_vcx::{
    errors::error::{AriesVcxError, AriesVcxErrorKind, VcxResult},
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
pub fn create_invitee(profile: Arc<ProfileHolder>, did_doc: String) -> VcxUniFFIResult<Arc<Connection>> {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Trace));
    block_on(async {
        let _did_doc: AriesDidDoc = serde_json::from_str(&did_doc)?;
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
