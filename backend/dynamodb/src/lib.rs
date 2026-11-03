//! Shared persistence adapter. The caller supplies the SDK client and table name;
//! production credentials and local endpoints are selected at the runtime boundary.

pub mod local;

use std::collections::HashMap;

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use mjers_table::Table;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the room changed before this write")]
    Conflict,
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
        result.item.map(|item| decode(id, item)).transpose()
    }

    /// Room-code collisions never overwrite the existing room.
    pub async fn create(
        &self,
        table: &Table,
    ) -> Result<(), StoreError> {
        self.write(table, None).await
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
        self.write(table, Some(expected_version)).await
    }

    async fn write(
        &self,
        table: &Table,
        expected_version: Option<u64>,
    ) -> Result<(), StoreError> {
        // JSON is kept as a string so all u64 seeds survive round trips exactly.
        // The indexed identity and CAS version remain native DynamoDB attributes.
        let mut request = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .item("table_id", AttributeValue::S(table.id().into()))
            .item("version", AttributeValue::N(table.version().to_string()))
            .item("document", AttributeValue::S(serde_json::to_string(table)?));
        request = if let Some(expected) = expected_version {
            request
                .condition_expression("attribute_exists(table_id) AND #version = :expected")
                .expression_attribute_names("#version", "version")
                .expression_attribute_values(":expected", AttributeValue::N(expected.to_string()))
        } else {
            request.condition_expression("attribute_not_exists(table_id)")
        };
        match request.send().await {
            Ok(_) => Ok(()),
            Err(error) if error.as_service_error().is_some_and(|error| error.is_conditional_check_failed_exception()) => Err(StoreError::Conflict),
            Err(error) => Err(StoreError::request(error)),
        }
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
