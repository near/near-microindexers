use serde::{Deserialize, Serialize};
use serde_json::json;

/// We want to store permission field more explicitly so we are making copy of nearcore struct
/// to change serde parameters of serialization.
#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub(crate) struct AccessKeyView {
    pub nonce: near_indexer_primitives::types::Nonce,
    pub permission: AccessKeyPermissionView,
}

impl From<&near_indexer_primitives::views::AccessKeyView> for AccessKeyView {
    fn from(access_key_view: &near_indexer_primitives::views::AccessKeyView) -> Self {
        Self {
            nonce: access_key_view.nonce,
            permission: access_key_view.permission.clone().into(),
        }
    }
}

/// This is a enum we want to store more explicitly, so we copy it from nearcore and provide
/// different serde representation settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) enum AccessKeyPermissionView {
    FunctionCall {
        allowance: Option<String>,
        receiver_id: String,
        method_names: Vec<String>,
    },
    FullAccess,
}

impl From<near_indexer_primitives::views::AccessKeyPermissionView> for AccessKeyPermissionView {
    fn from(permission: near_indexer_primitives::views::AccessKeyPermissionView) -> Self {
        match permission {
            near_indexer_primitives::views::AccessKeyPermissionView::FullAccess => Self::FullAccess,
            near_indexer_primitives::views::AccessKeyPermissionView::FunctionCall {
                allowance,
                receiver_id,
                method_names,
            } => Self::FunctionCall {
                allowance: allowance.map(|v| v.to_string()),
                receiver_id: receiver_id.escape_default().to_string(),
                method_names: method_names
                    .into_iter()
                    .map(|method_name| method_name.escape_default().to_string())
                    .collect(),
            },
        }
    }
}

pub(crate) fn extract_action_type_and_value_from_action_view(
    action_view: &near_indexer_primitives::views::ActionView,
) -> (String, serde_json::Value) {
    match action_view {
        near_indexer_primitives::views::ActionView::CreateAccount => {
            ("CREATE_ACCOUNT".to_string(), json!({}))
        }
        near_indexer_primitives::views::ActionView::DeployContract { code } => (
            "DEPLOY_CONTRACT".to_string(),
            json!({
                "code_sha256":  hex::encode(
                    base64::decode(code).expect("code expected to be encoded to base64")
                )
            }),
        ),
        near_indexer_primitives::views::ActionView::FunctionCall {
            method_name,
            args,
            gas,
            deposit,
        } => {
            let mut arguments = json!({
                "method_name": method_name.escape_default().to_string(),
                "args_base64": args,
                "gas": gas,
                "deposit": deposit.to_string(),
            });

            // During denormalization of action_receipt_actions table we wanted to try to decode
            // args which is base64 encoded in case if it is a JSON object and put them near initial
            // args_base64
            // See for reference https://github.com/near/near-indexer-for-explorer/issues/87
            if let Ok(decoded_args) = base64::decode(args) {
                if let Ok(mut args_json) = serde_json::from_slice(&decoded_args) {
                    escape_json(&mut args_json);
                    arguments["args_json"] = args_json;
                }
            }

            ("FUNCTION_CALL".to_string(), arguments)
        }
        near_indexer_primitives::views::ActionView::Transfer { deposit } => (
            "TRANSFER".to_string(),
            json!({ "deposit": deposit.to_string() }),
        ),
        near_indexer_primitives::views::ActionView::Stake { stake, public_key } => (
            "STAKE".to_string(),
            json!({
                "stake": stake.to_string(),
                "public_key": public_key,
            }),
        ),
        near_indexer_primitives::views::ActionView::AddKey {
            public_key,
            access_key,
        } => (
            "ADD_KEY".to_string(),
            json!({
                "public_key": public_key,
                "access_key": crate::models::serializers::AccessKeyView::from(access_key),
            }),
        ),
        near_indexer_primitives::views::ActionView::DeleteKey { public_key } => (
            "DELETE_KEY".to_string(),
            json!({
                "public_key": public_key,
            }),
        ),
        near_indexer_primitives::views::ActionView::DeleteAccount { beneficiary_id } => (
            "DELETE_ACCOUNT".to_string(),
            json!({
                "beneficiary_id": beneficiary_id,
            }),
        ),
    }
}

/// This function will modify the JSON escaping the values
/// We can not store data with null-bytes in TEXT or JSONB fields
/// of PostgreSQL
/// ref: https://www.commandprompt.com/blog/null-characters-workarounds-arent-good-enough/
fn escape_json(object: &mut serde_json::Value) {
    match object {
        serde_json::Value::Object(ref mut value) => {
            for (_key, val) in value {
                escape_json(val);
            }
        }
        serde_json::Value::Array(ref mut values) => {
            for element in values.iter_mut() {
                escape_json(element)
            }
        }
        serde_json::Value::String(ref mut value) => *value = value.escape_default().to_string(),
        _ => {}
    }
}
