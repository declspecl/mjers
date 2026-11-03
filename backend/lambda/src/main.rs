use lambda_runtime::{
    Error,
    LambdaEvent,
    service_fn,
};
use mjers_dynamodb::connections::ConnectionStore;
use mjers_dynamodb::limits::RateLimiter;
use mjers_dynamodb::{
    TableStore,
    epoch_seconds,
};
use mjers_lambda::Gateway;
use mjers_lambda::event::GatewayEvent;
use mjers_service::RoomService;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let tables = std::env::var("TABLES_TABLE_NAME")?;
    let connections = std::env::var("CONNECTIONS_TABLE_NAME")?;
    let endpoint = std::env::var("WEBSOCKET_MANAGEMENT_ENDPOINT")?;
    let origin = std::env::var("SITE_ORIGIN")?;
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let database = aws_sdk_dynamodb::Client::new(&config);
    let management = aws_sdk_apigatewaymanagement::Client::from_conf(aws_sdk_apigatewaymanagement::config::Builder::from(&config).endpoint_url(endpoint).build());
    let gateway = Gateway::new(
        RoomService::new(TableStore::new(database.clone(), tables)),
        ConnectionStore::new(database.clone(), connections.clone()),
        RateLimiter::new(database, connections),
        management,
        origin,
        epoch_seconds,
    );
    lambda_runtime::run(service_fn(|event: LambdaEvent<GatewayEvent>| async { Ok::<_, Error>(gateway.handle(event.payload).await) })).await
}
