use std::fmt::Display;

use uniffi::UnexpectedUniFFICallbackError;

pub type VcxUniFFIResult<T> = Result<T, VcxUniFFIError>;

// I've been super lazy here and only defined two types. But there
// can/should be effectively 1-to-1 mapping with Aries_VCX errors
#[derive(Debug, thiserror::Error)]

pub enum VcxUniFFIError {
    #[error("An AriesVCX error occured. More Info: {}", error_msg)]
    AriesVcxError { error_msg: String },
    #[error("A serialization error occurred. Check your inputs. More Info: {}", error_msg)]
    SerializationError { error_msg: String },
    #[error("An unexpected internal error occured. More Info: {}", error_msg)]
    InternalError { error_msg: String },
}

#[derive(Debug)]
pub enum NativeError {
    InternalError
}
impl Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InternalError in Callback")
    }
}

impl From<UnexpectedUniFFICallbackError> for NativeError {
    fn from(_value: UnexpectedUniFFICallbackError) -> Self {
       Self::InternalError
    }
}
