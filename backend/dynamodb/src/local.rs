//! DynamoDB Local configuration and schema setup, shared by the dev server and
//! integration tests. Production constructs its own client and provisions via CDK.

use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::config::{
    BehaviorVersion,
    Credentials,
    Region,
};
use aws_sdk_dynamodb::types::{
    AttributeDefinition,
    BillingMode,
    KeySchemaElement,
    KeyType,
    ScalarAttributeType,
};

use crate::StoreError;

pub fn client(endpoint: &str) -> Client {
    Client::from_conf(
        aws_sdk_dynamodb::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("local", "local", None, None, "dynamodb-local"))
            .build(),
    )
}

pub async fn ensure_table(
    client: &Client,
    name: &str,
) -> Result<(), StoreError> {
    match client.describe_table().table_name(name).send().await {
        Ok(result) => {
            let keys = result.table.as_ref().and_then(|table| table.key_schema.as_deref()).unwrap_or_default();
            if keys.len() != 1 || keys[0].attribute_name() != "table_id" || keys[0].key_type() != &KeyType::Hash {
                return Err(StoreError::InvalidRecord("expected a table_id partition key and no sort key"));
            }
            return Ok(());
        },
        Err(error) if error.as_service_error().is_some_and(|error| error.is_resource_not_found_exception()) => {},
        Err(error) => return Err(StoreError::request(error)),
    }
    let attribute = AttributeDefinition::builder()
        .attribute_name("table_id")
        .attribute_type(ScalarAttributeType::S)
        .build()
        .map_err(StoreError::request)?;
    let key = KeySchemaElement::builder().attribute_name("table_id").key_type(KeyType::Hash).build().map_err(StoreError::request)?;
    match client
        .create_table()
        .table_name(name)
        .attribute_definitions(attribute)
        .key_schema(key)
        .billing_mode(BillingMode::PayPerRequest)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        // Another local server may have provisioned the same table concurrently.
        Err(error) if error.as_service_error().is_some_and(|error| error.is_resource_in_use_exception()) => Ok(()),
        Err(error) => Err(StoreError::request(error)),
    }
}
