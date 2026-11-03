//! Shared persistence adapter. The caller supplies the SDK client and table name;
//! production credentials and local endpoints are selected at the runtime boundary.

pub mod connections;
pub mod limits;
pub mod local;

use std::collections::HashMap;

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::{
    AttributeValue,
    Put,
    TransactWriteItem,
};
use mjers_table::{
    HandRecord,
    Table,
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the room changed before this write")]
    Conflict,
    #[error("request limit reached")]
    RateLimited,
    #[error("connection ownership changed")]
    ConnectionLost,
    #[error("invalid persisted room: {0}")]
    InvalidRecord(&'static str),
    #[error("invalid room document")]
    Document(#[from] serde_json::Error),
    #[error("DynamoDB request failed")]
    Request(#[source] Box<dyn std::error::Error + Send + Sync>),
}
impl StoreError {
    fn request(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Request(Box::new(error))
    }
}

#[derive(Clone)]
pub struct TableStore {
    client: Client,
    table_name: String,
}
impl TableStore {
    pub fn new(
        client: Client,
        table_name: String,
    ) -> Self {
        Self { client, table_name }
    }

    /// Strong consistency is necessary before evaluating every command.
    pub async fn load(
        &self,
        id: &str,
    ) -> Result<Option<Table>, StoreError> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("table_id", AttributeValue::S(id.into()))
            .consistent_read(true)
            .send()
            .await
            .map_err(StoreError::request)?;
        if let Some(item) = result.item {
            if expired(&item)? {
                return Ok(None);
            }
            return decode(id, item).map(Some);
        }
        Ok(None)
    }

    /// Room-code collisions never overwrite the existing room.
    pub async fn create(
        &self,
        table: &Table,
    ) -> Result<(), StoreError> {
        self.write(table, None, None).await
    }

    /// Compare-and-set is the only mutation path. A conflict must cause the
    /// caller to reload and re-evaluate the command, not overwrite newer data.
    pub async fn save(
        &self,
        table: &Table,
        expected_version: u64,
    ) -> Result<(), StoreError> {
        if expected_version.checked_add(1) != Some(table.version()) {
            return Err(StoreError::InvalidRecord("a write must advance the version once"));
        }
        self.write(table, Some(expected_version), None).await
    }

    pub async fn save_connected(
        &self,
        table: &Table,
        expected_version: u64,
        guard: &connections::ConnectionGuard,
    ) -> Result<(), StoreError> {
        if expected_version.checked_add(1) != Some(table.version()) {
            return Err(StoreError::InvalidRecord("a write must advance the version once"));
        }
        self.write(table, Some(expected_version), Some(guard)).await
    }

    async fn write(
        &self,
        table: &Table,
        expected_version: Option<u64>,
        guard: Option<&connections::ConnectionGuard>,
    ) -> Result<(), StoreError> {
        let now = epoch_seconds()?;
        let document = serde_json::to_string(table)?;
        if document.len() > 380_000 {
            return Err(StoreError::InvalidRecord("room document exceeds the storage limit"));
        }
        let item = HashMap::from([
            ("table_id".into(), AttributeValue::S(table.id().into())),
            ("version".into(), AttributeValue::N(table.version().to_string())),
            ("document".into(), AttributeValue::S(document)),
            ("ttl".into(), AttributeValue::N((now + 86_400).to_string())),
        ]);
        // Use a transaction so a completed hand and its score commit together.
        let mut put = Put::builder().table_name(&self.table_name).set_item(Some(item));
        put = if let Some(expected) = expected_version {
            put.condition_expression("attribute_exists(table_id) AND #version = :expected AND (attribute_not_exists(#ttl) OR #ttl > :now)")
                .expression_attribute_names("#version", "version")
                .expression_attribute_names("#ttl", "ttl")
                .expression_attribute_values(":expected", AttributeValue::N(expected.to_string()))
                .expression_attribute_values(":now", AttributeValue::N(now.to_string()))
        } else {
            put.condition_expression("attribute_not_exists(table_id)")
        };
        let mut request = self
            .client
            .transact_write_items()
            .transact_items(TransactWriteItem::builder().put(put.build().map_err(StoreError::request)?).build());
        if let Some(guard) = guard {
            request = request.transact_items(TransactWriteItem::builder().condition_check(guard.condition(now)?).build());
        }
        if let Some(hand) = table.completed_hand().map_err(|_| StoreError::InvalidRecord("invalid completed hand"))? {
            let archive = Put::builder()
                .table_name(&self.table_name)
                .item("table_id", AttributeValue::S(format!("hand#{}#{}", table.id(), table.hand_number())))
                .item("document", AttributeValue::S(serde_json::to_string(hand)?))
                .item("ttl", AttributeValue::N((now + 604_800).to_string()))
                .build()
                .map_err(StoreError::request)?;
            request = request.transact_items(TransactWriteItem::builder().put(archive).build());
        }
        match request.send().await {
            Ok(_) => Ok(()),
            Err(error)
                if guard.is_some()
                    && error.as_service_error().is_some_and(|error| match error {
                        aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsError::TransactionCanceledException(error) => {
                            error.cancellation_reasons().get(1).is_some_and(|reason| reason.code() == Some("ConditionalCheckFailed"))
                        },
                        _ => false,
                    }) =>
            {
                Err(StoreError::ConnectionLost)
            },
            Err(error) if error.as_service_error().is_some_and(transaction_conflict) => Err(StoreError::Conflict),
            Err(error) => Err(StoreError::request(error)),
        }
    }

    /// Server-only replay archive. Callers must authorize access through the room.
    pub async fn load_hand(
        &self,
        id: &str,
        number: u32,
    ) -> Result<Option<HandRecord>, StoreError> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("table_id", AttributeValue::S(format!("hand#{id}#{number}")))
            .consistent_read(true)
            .send()
            .await
            .map_err(StoreError::request)?;
        let Some(item) = result.item else {
            return Ok(None);
        };
        if expired(&item)? {
            return Ok(None);
        }
        let document = item.get("document").and_then(|v| v.as_s().ok()).ok_or(StoreError::InvalidRecord("missing archive"))?;
        Ok(Some(serde_json::from_str(document)?))
    }
}

fn decode(
    id: &str,
    item: HashMap<String, AttributeValue>,
) -> Result<Table, StoreError> {
    let document = item.get("document").and_then(|value| value.as_s().ok()).ok_or(StoreError::InvalidRecord("missing document"))?;
    let table: Table = serde_json::from_str(document)?;
    let version = item.get("version").and_then(|value| value.as_n().ok()).and_then(|value| value.parse::<u64>().ok());
    if table.id() != id || version != Some(table.version()) {
        return Err(StoreError::InvalidRecord("identity or version mismatch"));
    }
    Ok(table)
}

pub fn epoch_seconds() -> Result<u64, StoreError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(StoreError::request)
}
fn expired(item: &HashMap<String, AttributeValue>) -> Result<bool, StoreError> {
    match item.get("ttl") {
        None => Ok(false), // Existing local records gain a TTL on their next write.
        Some(ttl) => Ok(ttl.as_n().ok().and_then(|n| n.parse::<u64>().ok()).ok_or(StoreError::InvalidRecord("invalid expiry"))? <= epoch_seconds()?),
    }
}
fn transaction_conflict(error: &aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsError) -> bool {
    match error {
        aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsError::TransactionCanceledException(error) => error
            .cancellation_reasons()
            .iter()
            .any(|reason| matches!(reason.code(), Some("ConditionalCheckFailed" | "TransactionConflict"))),
        _ => false,
    }
}
