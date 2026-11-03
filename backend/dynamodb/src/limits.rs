//! Atomic fixed-window request counters. Raw IP addresses are not stored.

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use sha2::{
    Digest,
    Sha256,
};

use crate::StoreError;

#[derive(Clone)]
pub struct RateLimiter {
    client: Client,
    table_name: String,
}
impl RateLimiter {
    pub fn new(
        client: Client,
        table_name: String,
    ) -> Self {
        Self { client, table_name }
    }
    pub async fn check(
        &self,
        scope: &str,
        limit: u32,
        now: u64,
    ) -> Result<(), StoreError> {
        let key = format!("limit#{:x}#{}", Sha256::digest(scope.as_bytes()), now / 60);
        let result = self
            .client
            .update_item()
            .table_name(&self.table_name)
            .key("connection_id", AttributeValue::S(key))
            .update_expression("SET #count = if_not_exists(#count, :zero) + :one, #ttl = :ttl")
            .condition_expression("attribute_not_exists(#count) OR #count < :limit")
            .expression_attribute_names("#count", "requests")
            .expression_attribute_names("#ttl", "ttl")
            .expression_attribute_values(":zero", AttributeValue::N("0".into()))
            .expression_attribute_values(":one", AttributeValue::N("1".into()))
            .expression_attribute_values(":limit", AttributeValue::N(limit.to_string()))
            .expression_attribute_values(":ttl", AttributeValue::N(((now / 60 + 2) * 60).to_string()))
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) if error.as_service_error().is_some_and(|e| e.is_conditional_check_failed_exception()) => Err(StoreError::RateLimited),
            Err(error) => Err(StoreError::request(error)),
        }
    }
}
