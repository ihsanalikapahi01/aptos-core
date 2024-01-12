// Copyright © Aptos Foundation

use crate::{
    jwks::{rsa::RSA_JWK, unsupported::UnsupportedJWK},
    move_any::{Any as MoveAny, AsMoveAny},
    move_utils::as_move_value::AsMoveValue,
};
use anyhow::anyhow;
use aptos_crypto_derive::{BCSCryptoHash, CryptoHasher};
use move_core_types::value::{MoveStruct, MoveValue};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

/// Reflection of Move type `0x1::jwks::JWK`.
/// When you load an on-chain config that contains some JWK(s), the JWK will be of this type.
/// When you call a Move function from rust that takes some JWKs as input, pass in JWKs of this type.
/// Otherwise, it is recommended to convert this to the rust enum `JWK` below for better rust experience.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, CryptoHasher, BCSCryptoHash)]
pub struct JWKMoveStruct {
    pub variant: MoveAny,
}

impl Debug for JWKMoveStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let jwk = JWK::try_from(self.clone());
        f.debug_struct("JWKMoveStruct")
            .field("variant", &jwk)
            .finish()
    }
}

impl AsMoveValue for JWKMoveStruct {
    fn as_move_value(&self) -> MoveValue {
        MoveValue::Struct(MoveStruct::Runtime(vec![self.variant.as_move_value()]))
    }
}

/// The JWK type that can be converted from/to `JWKMoveStruct` but easier to use in rust.
#[derive(Debug, PartialEq)]
pub enum JWK {
    RSA(RSA_JWK),
    Unsupported(UnsupportedJWK),
}

impl From<serde_json::Value> for JWK {
    fn from(value: serde_json::Value) -> Self {
        match RSA_JWK::try_from(&value) {
            Ok(rsa) => Self::RSA(rsa),
            Err(_) => {
                let unsupported = UnsupportedJWK::from(value);
                Self::Unsupported(unsupported)
            },
        }
    }
}

impl From<JWK> for JWKMoveStruct {
    fn from(jwk: JWK) -> Self {
        let variant = match jwk {
            JWK::RSA(variant) => variant.as_move_any(),
            JWK::Unsupported(variant) => variant.as_move_any(),
        };
        JWKMoveStruct { variant }
    }
}

impl TryFrom<JWKMoveStruct> for JWK {
    type Error = anyhow::Error;

    fn try_from(value: JWKMoveStruct) -> Result<Self, Self::Error> {
        match value.variant.type_name.as_str() {
            RSA_JWK::MOVE_TYPE_NAME => {
                let rsa_jwk = MoveAny::unpack(RSA_JWK::MOVE_TYPE_NAME, value.variant).unwrap();
                Ok(Self::RSA(rsa_jwk))
            },
            UnsupportedJWK::MOVE_TYPE_NAME => {
                let unsupported_jwk =
                    MoveAny::unpack(UnsupportedJWK::MOVE_TYPE_NAME, value.variant).unwrap();
                Ok(Self::Unsupported(unsupported_jwk))
            },
            _ => Err(anyhow!(
                "convertion from jwk move struct to jwk failed with unknown variant"
            )),
        }
    }
}

#[cfg(test)]
mod tests;