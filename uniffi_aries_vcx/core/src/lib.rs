uniffi::include_scaffolding!("vcx");

pub mod core;
pub mod errors;
pub mod handlers;
pub mod runtime;

use crate::core::profile::*;
use crate::errors::error::*;
use crate::handlers::*;
use aries_vcx::{aries_vcx_core::indy::wallet::WalletConfig, protocols::connection::pairwise_info::PairwiseInfo};

use handlers::{
    connection::{connection::*, *},
    issuance::{issuance::*, *},
};
