import { CfnOutput, Duration, Fn, RemovalPolicy, Stack, Tags, type StackProps } from "aws-cdk-lib";
import { CorsHttpMethod, HttpApi, HttpMethod, WebSocketApi, WebSocketStage, CfnStage } from "aws-cdk-lib/aws-apigatewayv2";
import { HttpLambdaIntegration, WebSocketLambdaIntegration } from "aws-cdk-lib/aws-apigatewayv2-integrations";
import { PolicyStatement } from "aws-cdk-lib/aws-iam";
import { Architecture, Code, Function, LoggingFormat, Runtime } from "aws-cdk-lib/aws-lambda";
import { LogGroup, RetentionDays } from "aws-cdk-lib/aws-logs";
import type { Construct } from "constructs";
import { RoomStorage } from "../constructs/room-storage.js";

export interface MultiplayerStackProps extends StackProps {
    readonly stage: "dev" | "prod";
    readonly siteOrigins: readonly string[];
    readonly code: Code;
}

export class MultiplayerStack extends Stack {
    constructor(scope: Construct, id: string, props: MultiplayerStackProps) {
        super(scope, id, props);
        Tags.of(this).add("project", "mjers");
        Tags.of(this).add("stage", props.stage);
        const storage = new RoomStorage(this, "Storage", props.stage);
        const sockets = new WebSocketApi(this, "Sockets", {
            apiName: `mjers-${props.stage}-sockets`,
            routeSelectionExpression: "$request.body.type",
        });
        const http = new HttpApi(this, "Http", {
            apiName: `mjers-${props.stage}-rooms`,
            corsPreflight: {
                allowOrigins: [...props.siteOrigins],
                allowMethods: [CorsHttpMethod.POST],
                allowHeaders: ["content-type"],
                maxAge: Duration.hours(1),
            },
        });
        const managementEndpoint = Fn.join("", [
            "https://",
            sockets.apiId,
            ".execute-api.",
            this.region,
            ".",
            this.urlSuffix,
            "/",
            props.stage,
        ]);
        const logGroup = new LogGroup(this, "RuntimeLogs", {
            retention: RetentionDays.ONE_WEEK,
            removalPolicy: RemovalPolicy.DESTROY,
        });
        const runtime = new Function(this, "Runtime", {
            functionName: `mjers-${props.stage}-backend`,
            runtime: Runtime.PROVIDED_AL2023,
            architecture: Architecture.ARM_64,
            handler: "bootstrap",
            code: props.code,
            memorySize: 512,
            timeout: Duration.seconds(25),
            reservedConcurrentExecutions: 8,
            loggingFormat: LoggingFormat.JSON,
            logGroup,
            environment: {
                TABLES_TABLE_NAME: storage.rooms.tableName,
                CONNECTIONS_TABLE_NAME: storage.connections.tableName,
                WEBSOCKET_MANAGEMENT_ENDPOINT: managementEndpoint,
                SITE_ORIGINS: props.siteOrigins.join(","),
            },
        });
        // Grant only operations used by the shared store; no scans or deletes of rooms.
        runtime.addToRolePolicy(
            new PolicyStatement({
                actions: ["dynamodb:GetItem", "dynamodb:PutItem"],
                resources: [storage.rooms.tableArn],
            }),
        );
        runtime.addToRolePolicy(
            new PolicyStatement({
                actions: [
                    "dynamodb:GetItem",
                    "dynamodb:PutItem",
                    "dynamodb:UpdateItem",
                    "dynamodb:DeleteItem",
                    "dynamodb:ConditionCheckItem",
                ],
                resources: [storage.connections.tableArn],
            }),
        );
        runtime.addToRolePolicy(
            new PolicyStatement({
                actions: ["execute-api:ManageConnections"],
                resources: [
                    this.formatArn({ service: "execute-api", resource: sockets.apiId, resourceName: `${props.stage}/*/@connections/*` }),
                ],
            }),
        );
        http.addRoutes({ path: "/tables", methods: [HttpMethod.POST], integration: new HttpLambdaIntegration("CreateRoom", runtime) });
        http.addRoutes({
            path: "/tables/{id}/join",
            methods: [HttpMethod.POST],
            integration: new HttpLambdaIntegration("JoinRoom", runtime),
        });
        const httpStage = http.defaultStage?.node.defaultChild;
        if (!(httpStage instanceof CfnStage)) throw new Error("HTTP default stage is required.");
        httpStage.defaultRouteSettings = { throttlingBurstLimit: 30, throttlingRateLimit: 10 };
        for (const route of ["$connect", "$disconnect", "$default"]) {
            sockets.addRoute(route, { integration: new WebSocketLambdaIntegration(`${route}Integration`, runtime) });
        }
        const socketStage = new WebSocketStage(this, "SocketStage", { webSocketApi: sockets, stageName: props.stage, autoDeploy: true });
        const socketResource = socketStage.node.defaultChild;
        if (!(socketResource instanceof CfnStage)) throw new Error("WebSocket stage is required.");
        socketResource.defaultRouteSettings = { throttlingBurstLimit: 100, throttlingRateLimit: 50, dataTraceEnabled: false };
        new CfnOutput(this, "HttpEndpoint", { value: http.apiEndpoint });
        new CfnOutput(this, "WebSocketEndpoint", { value: socketStage.url });
        new CfnOutput(this, "RoomsTable", { value: storage.rooms.tableName });
        new CfnOutput(this, "ConnectionsTable", { value: storage.connections.tableName });
    }
}
