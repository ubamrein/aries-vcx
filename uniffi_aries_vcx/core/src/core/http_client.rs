use aries_vcx::{
    agency_client::httpclient::post_message,
    errors::error::{AriesVcxError, AriesVcxErrorKind, VcxResult},
    transport::Transport,
};
use async_trait::async_trait;
use url::Url;

use crate::errors::error::{VcxUniFFIResult, NativeError};

pub struct HttpClient;

#[async_trait]
impl Transport for HttpClient {
    async fn send_message(&self, msg: Vec<u8>, service_endpoint: Url) -> VcxResult<()> {
        post_message(msg, service_endpoint).await?;
        Ok(())
    }
}

pub trait NativeTransport: Send + Sync {
    fn send_message(&self, msg: Vec<u8>, service_endpoint: String) -> Result<(), NativeError>;
}

pub struct NativeClient{ client: Box<dyn NativeTransport>}

impl NativeClient {
    pub fn new(native_transport: Box<dyn NativeTransport>) -> Self {
        Self{ client: native_transport}
    }
}

#[async_trait]
impl Transport for NativeClient {
    async fn send_message(&self, msg: Vec<u8>, service_endpoint: Url) -> VcxResult<()> {
        self.client
            .send_message(msg, service_endpoint.to_string())
            .map_err(|e| AriesVcxError::from_msg(AriesVcxErrorKind::IOError, e))?;
        Ok(())
    }
}


pub fn create_native_client(native_client: Box<dyn NativeTransport>) -> NativeClient {
    NativeClient{ client: native_client}
}
