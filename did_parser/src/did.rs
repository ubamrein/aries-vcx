use std::convert::TryFrom;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{error::ParseError, utils::parse::parse_did_method_id, DidRange};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Did {
    did: String,
    method: DidRange,
    id: DidRange,
}

impl Did {
    pub fn parse(did: String) -> Result<Self, ParseError> {
        let (_, method, id) = parse_did_method_id(&did)?;

        if id.end > did.len() {
            return Err(ParseError::InvalidInput("Invalid DID"));
        }

        Ok(Self { did, method, id })
    }

    pub fn did(&self) -> &str {
        self.did.as_ref()
    }

    pub fn method(&self) -> &str {
        self.did[self.method.clone()].as_ref()
    }

    pub fn id(&self) -> &str {
        self.did[self.id.clone()].as_ref()
    }

    pub(crate) fn from_parts(did: String, method: DidRange, id: DidRange) -> Self {
        Self { did, method, id }
    }
}

impl TryFrom<String> for Did {
    type Error = ParseError;

    fn try_from(did: String) -> Result<Self, Self::Error> {
        Self::parse(did)
    }
}

impl FromStr for Did {
    type Err = ParseError;

    fn from_str(did: &str) -> Result<Self, Self::Err> {
        Self::parse(did.to_string())
    }
}

impl Display for Did {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.did)
    }
}

impl Default for Did {
    fn default() -> Self {
        Self {
            did: "did:example:123456789abcdefghi".to_string(),
            method: 4..11,
            id: 12..30,
        }
    }
}

impl Serialize for Did {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.did())
    }
}

impl<'de> Deserialize<'de> for Did {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let did = String::deserialize(deserializer)?;
        Self::parse(did).map_err(serde::de::Error::custom)
    }
}
