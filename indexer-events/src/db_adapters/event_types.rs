use near_primitives::types::AccountId;
use serde::{Deserialize, Serialize};

use crate::db_adapters::numeric_types::U128;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "standard")]
#[serde(rename_all = "snake_case")]
pub(crate) enum NearEvent {
    Nep141(Nep141Event),
    Nep171(Nep171Event),
}

// *** NEP-141 FT ***
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Nep141Event {
    pub version: String,
    #[serde(flatten)]
    pub event_kind: Nep141EventKind,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Nep141EventKind {
    FtMint(Vec<FtMintData>),
    FtTransfer(Vec<FtTransferData>),
    FtBurn(Vec<FtBurnData>),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FtMintData {
    pub owner_id: AccountId,
    pub amount: U128,
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FtTransferData {
    pub old_owner_id: AccountId,
    pub new_owner_id: AccountId,
    pub amount: U128,
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FtBurnData {
    pub owner_id: AccountId,
    pub amount: U128,
    pub memo: Option<String>,
}

// *** NEP-171 NFT ***
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Nep171Event {
    pub version: String,
    #[serde(flatten)]
    pub event_kind: Nep171EventKind,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Nep171EventKind {
    NftMint(Vec<NftMintData>),
    NftTransfer(Vec<NftTransferData>),
    NftBurn(Vec<NftBurnData>),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct NftMintData {
    pub owner_id: AccountId,
    pub token_ids: Vec<String>,
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct NftTransferData {
    pub authorized_id: Option<String>,
    pub old_owner_id: AccountId,
    pub new_owner_id: AccountId,
    pub token_ids: Vec<String>,
    pub memo: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct NftBurnData {
    pub authorized_id: Option<String>,
    pub owner_id: AccountId,
    pub token_ids: Vec<String>,
    pub memo: Option<String>,
}
