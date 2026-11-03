//! Socket and seat records commit atomically. Four strongly consistent seat
//! reads replace an eventually consistent table index for this bounded domain.

use std::collections::HashMap;

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::{
    AttributeValue,
    ConditionCheck,
    Delete,
    Put,
    TransactWriteItem,
};
use mjers_core::rules::seat::Seat;
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    StoreError,
    transaction_conflict,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub table: String,
    pub seat: Seat,
    pub expires_at: u64,
    pub ready: bool,
}
#[derive(Clone)]
pub struct ConnectionGuard {
    table_name: String,
    key: String,
    id: String,
}
impl ConnectionGuard {
    pub(crate) fn condition(
        &self,
        now: u64,
    ) -> Result<ConditionCheck, StoreError> {
        ConditionCheck::builder()
            .table_name(&self.table_name)
            .key("connection_id", AttributeValue::S(self.key.clone()))
            .condition_expression("socket_id = :id AND #ttl > :now")
            .expression_attribute_names("#ttl", "ttl")
            .expression_attribute_values(":id", AttributeValue::S(self.id.clone()))
            .expression_attribute_values(":now", AttributeValue::N(now.to_string()))
            .build()
            .map_err(StoreError::request)
    }
}
#[derive(Clone)]
pub struct ConnectionStore {
    client: Client,
    table_name: String,
}
impl ConnectionStore {
    pub fn new(
        client: Client,
        table_name: String,
    ) -> Self {
        Self { client, table_name }
    }
    pub fn guard(
        &self,
        connection: &Connection,
    ) -> ConnectionGuard {
        ConnectionGuard {
            table_name: self.table_name.clone(),
            key: seat_key(&connection.table, connection.seat),
            id: connection.id.clone(),
        }
    }
    async fn record(
        &self,
        key: String,
    ) -> Result<Option<Connection>, StoreError> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("connection_id", AttributeValue::S(key))
            .consistent_read(true)
            .send()
            .await
            .map_err(StoreError::request)?;
        result
            .item
            .map(|item| {
                let text = item.get("document").and_then(|v| v.as_s().ok()).ok_or(StoreError::InvalidRecord("missing connection"))?;
                let connection: Connection = serde_json::from_str(text)?;
                if connection.seat.index() >= 4 {
                    return Err(StoreError::InvalidRecord("invalid connection seat"));
                }
                Ok(connection)
            })
            .transpose()
    }
    pub async fn load(
        &self,
        id: &str,
        now: u64,
    ) -> Result<Option<Connection>, StoreError> {
        let Some(socket) = self.record(format!("socket#{id}")).await? else {
            return Ok(None);
        };
        let current = self.record(seat_key(&socket.table, socket.seat)).await?;
        Ok(current.filter(|current| current.id == id && current.expires_at > now))
    }
    pub async fn list(
        &self,
        table: &str,
        now: u64,
    ) -> Result<Vec<Connection>, StoreError> {
        let mut connections = Vec::new();
        for seat in Seat::ALL {
            if let Some(connection) = self.record(seat_key(table, seat)).await?
                && connection.expires_at > now
                && connection.ready
            {
                connections.push(connection);
            }
        }
        Ok(connections)
    }
    /// Returns the replaced id, so the adapter can close the old socket.
    pub async fn connect(
        &self,
        connection: &Connection,
    ) -> Result<Option<String>, StoreError> {
        let key = seat_key(&connection.table, connection.seat);
        let old = self.record(key.clone()).await?;
        if old.as_ref().is_some_and(|old| old.id == connection.id) {
            return Ok(None);
        }
        let mut seat = self.put(key, connection)?;
        seat = if let Some(old) = &old {
            seat.condition_expression("socket_id = :old").expression_attribute_values(":old", AttributeValue::S(old.id.clone()))
        } else {
            seat.condition_expression("attribute_not_exists(connection_id)")
        };
        let socket = self.put(format!("socket#{}", connection.id), connection)?.condition_expression("attribute_not_exists(connection_id)");
        let mut request = self
            .client
            .transact_write_items()
            .transact_items(TransactWriteItem::builder().put(seat.build().map_err(StoreError::request)?).build())
            .transact_items(TransactWriteItem::builder().put(socket.build().map_err(StoreError::request)?).build());
        if let Some(old) = &old {
            let delete = Delete::builder()
                .table_name(&self.table_name)
                .key("connection_id", AttributeValue::S(format!("socket#{}", old.id)))
                .build()
                .map_err(StoreError::request)?;
            request = request.transact_items(TransactWriteItem::builder().delete(delete).build());
        }
        match request.send().await {
            Ok(_) => Ok(old.map(|old| old.id)),
            Err(error) if error.as_service_error().is_some_and(transaction_conflict) => Err(StoreError::Conflict),
            Err(error) => Err(StoreError::request(error)),
        }
    }
    pub async fn mark_ready(
        &self,
        connection: &Connection,
    ) -> Result<(), StoreError> {
        let mut ready = connection.clone();
        ready.ready = true;
        let result = self
            .client
            .update_item()
            .table_name(&self.table_name)
            .key("connection_id", AttributeValue::S(seat_key(&connection.table, connection.seat)))
            .update_expression("SET document = :document")
            .condition_expression("socket_id = :id")
            .expression_attribute_values(":id", AttributeValue::S(connection.id.clone()))
            .expression_attribute_values(":document", AttributeValue::S(serde_json::to_string(&ready)?))
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) if error.as_service_error().is_some_and(|e| e.is_conditional_check_failed_exception()) => Err(StoreError::Conflict),
            Err(error) => Err(StoreError::request(error)),
        }
    }
    /// A late disconnect from a replaced socket cannot remove its successor.
    pub async fn disconnect(
        &self,
        id: &str,
    ) -> Result<Option<Connection>, StoreError> {
        for _ in 0..4 {
            let Some(connection) = self.record(format!("socket#{id}")).await? else {
                return Ok(None);
            };
            let seat = Delete::builder()
                .table_name(&self.table_name)
                .key("connection_id", AttributeValue::S(seat_key(&connection.table, connection.seat)))
                .condition_expression("socket_id = :id")
                .expression_attribute_values(":id", AttributeValue::S(id.into()))
                .build()
                .map_err(StoreError::request)?;
            let socket = Delete::builder()
                .table_name(&self.table_name)
                .key("connection_id", AttributeValue::S(format!("socket#{id}")))
                .build()
                .map_err(StoreError::request)?;
            match self
                .client
                .transact_write_items()
                .transact_items(TransactWriteItem::builder().delete(seat).build())
                .transact_items(TransactWriteItem::builder().delete(socket).build())
                .send()
                .await
            {
                Ok(_) => return Ok(Some(connection)),
                Err(error) if error.as_service_error().is_some_and(transaction_conflict) => continue,
                Err(error) => return Err(StoreError::request(error)),
            }
        }
        Err(StoreError::Conflict)
    }
    fn put(
        &self,
        key: String,
        connection: &Connection,
    ) -> Result<aws_sdk_dynamodb::types::builders::PutBuilder, StoreError> {
        let item = HashMap::from([
            ("connection_id".into(), AttributeValue::S(key)),
            ("socket_id".into(), AttributeValue::S(connection.id.clone())),
            ("document".into(), AttributeValue::S(serde_json::to_string(connection)?)),
            ("ttl".into(), AttributeValue::N(connection.expires_at.to_string())),
        ]);
        Ok(Put::builder().table_name(&self.table_name).set_item(Some(item)))
    }
}
fn seat_key(
    table: &str,
    seat: Seat,
) -> String {
    format!("seat#{table}#{}", seat.index())
}
