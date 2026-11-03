//! API Gateway HTTP API v2 and WebSocket proxy v1 event fields used by this app.
//! Unused platform fields are intentionally ignored; client bodies stay strict.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mjers_service::ServiceError;
use mjers_table::TableError;
use mjers_table::protocol::ErrorCode;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEvent {
    #[serde(default)]
    pub request_context: RequestContext,
    #[serde(default)]
    pub raw_path: String,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub query_string_parameters: Option<HashMap<String, String>>,
    pub body: Option<String>,
    #[serde(default)]
    pub is_base64_encoded: bool,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestContext {
    pub http: Option<HttpContext>,
    pub route_key: Option<String>,
    pub connection_id: Option<String>,
    #[serde(default)]
    pub identity: Option<Identity>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpContext {
    pub method: String,
    pub source_ip: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    #[serde(default)]
    pub source_ip: String,
}
impl GatewayEvent {
    pub fn header(
        &self,
        name: &str,
    ) -> Option<&str> {
        self.headers.as_ref()?.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
    pub fn body(&self) -> Result<String, ServiceError> {
        let body = self.body.as_deref().unwrap_or_default();
        if body.len() > 5500 {
            return Err(invalid("The message is too large."));
        }
        let body = if self.is_base64_encoded {
            String::from_utf8(STANDARD.decode(body).map_err(|_| invalid("Invalid message encoding."))?).map_err(|_| invalid("Invalid message encoding."))?
        } else {
            body.into()
        };
        if body.len() > 4096 {
            return Err(invalid("The message is too large."));
        }
        Ok(body)
    }
    pub fn query(
        &self,
        key: &str,
    ) -> Result<&str, ServiceError> {
        self.query_string_parameters
            .as_ref()
            .and_then(|query| query.get(key))
            .map(String::as_str)
            .ok_or_else(|| invalid("The room code and seat token are required."))
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub is_base64_encoded: bool,
}
impl GatewayResponse {
    pub fn json(
        status: u16,
        value: &impl Serialize,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            status_code: status,
            headers: HashMap::from([("content-type".into(), "application/json".into()), ("cache-control".into(), "no-store".into())]),
            body: serde_json::to_string(value).map_err(|_| ServiceError::Runtime)?,
            is_base64_encoded: false,
        })
    }
}
pub fn invalid(message: &str) -> ServiceError {
    TableError {
        code: ErrorCode::InvalidRoom,
        message: message.into(),
    }
    .into()
}
