import assert from "node:assert/strict";
import { test } from "node:test";
import { App } from "aws-cdk-lib";
import { Template, Match } from "aws-cdk-lib/assertions";
import { Code } from "aws-cdk-lib/aws-lambda";
import { MultiplayerStack } from "../src/stacks/multiplayer-stack.js";
import { readDeploymentConfig } from "../src/config.js";

test("backend stack wires native Lambda, HTTP, sockets, conditional storage and a budget", () => {
    const app = new App();
    const stack = new MultiplayerStack(app, "test-backend", {
        stage: "prod",
        siteOrigin: "https://game.example",
        budgetEmail: "owner@example.com",
        code: Code.fromCfnParameters(),
    });
    const template = Template.fromStack(stack);
    template.resourceCountIs("AWS::DynamoDB::Table", 2);
    template.hasResourceProperties("AWS::DynamoDB::Table", {
        KeySchema: [{ AttributeName: "table_id", KeyType: "HASH" }],
        BillingMode: "PAY_PER_REQUEST",
        TimeToLiveSpecification: { AttributeName: "ttl", Enabled: true },
    });
    template.hasResourceProperties("AWS::Lambda::Function", {
        Runtime: "provided.al2023",
        Architectures: ["arm64"],
        Handler: "bootstrap",
        Timeout: 25,
        ReservedConcurrentExecutions: 8,
        Environment: { Variables: Match.objectLike({ SITE_ORIGIN: "https://game.example" }) },
    });
    for (const route of ["POST /tables", "POST /tables/{id}/join", "$connect", "$disconnect", "$default"]) {
        template.hasResourceProperties("AWS::ApiGatewayV2::Route", { RouteKey: route, Target: Match.anyValue() });
    }
    template.hasResourceProperties("AWS::ApiGatewayV2::Api", {
        ProtocolType: "HTTP",
        CorsConfiguration: { AllowOrigins: ["https://game.example"], AllowMethods: ["POST"], AllowHeaders: ["content-type"], MaxAge: 3600 },
    });
    template.hasResourceProperties("AWS::ApiGatewayV2::Stage", {
        DefaultRouteSettings: { ThrottlingBurstLimit: 100, ThrottlingRateLimit: 50, DataTraceEnabled: false },
    });
    template.hasResourceProperties("AWS::Budgets::Budget", {
        Budget: { BudgetType: "COST", BudgetLimit: { Amount: 1, Unit: "USD" }, TimeUnit: "MONTHLY", BudgetName: "mjers-prod-monthly" },
        NotificationsWithSubscribers: Match.arrayWith([
            Match.objectLike({ Subscribers: [{ SubscriptionType: "EMAIL", Address: "owner@example.com" }] }),
        ]),
    });
    const resources = template.toJSON().Resources as Record<string, { Type: string; DeletionPolicy?: string; Properties?: unknown }>;
    for (const resource of Object.values(resources)) {
        if (resource.Type === "AWS::DynamoDB::Table") assert.equal(resource.DeletionPolicy, "Retain");
        assert.notEqual(resource.Type, "AWS::Scheduler::Schedule");
    }
    const policies = JSON.stringify(template.findResources("AWS::IAM::Policy"));
    assert.ok(policies.includes("execute-api:ManageConnections"));
    assert.ok(policies.includes("@connections/*"));
    assert.ok(policies.includes("dynamodb:ConditionCheckItem"));
    assert.ok(!policies.includes('"dynamodb:*"'));
    assert.ok(!policies.includes('"execute-api:*"'));
    assert.ok(!policies.includes("dynamodb:Scan"));
    // Synth checks the whole dependency graph, including API/Lambda cycles.
    app.synth();
});

test("deployment configuration requires explicit stage, origin and notification address", () => {
    assert.throws(() => readDeploymentConfig({}), /DEPLOYMENT_STAGE/);
    assert.throws(() => readDeploymentConfig({ DEPLOYMENT_STAGE: "prod" }), /SITE_ORIGIN/);
    assert.throws(() => readDeploymentConfig({ DEPLOYMENT_STAGE: "prod", SITE_ORIGIN: "https://game.example" }), /BUDGET_EMAIL/);
    assert.throws(
        () => readDeploymentConfig({ DEPLOYMENT_STAGE: "prod", SITE_ORIGIN: "https://game.example/path", BUDGET_EMAIL: "a@b.example" }),
        /origin/,
    );
    const config = readDeploymentConfig({ DEPLOYMENT_STAGE: "dev", SITE_ORIGIN: "http://localhost:5173/", BUDGET_EMAIL: "a@b.example" });
    assert.equal(config.siteOrigin, "http://localhost:5173");
    assert.ok(config.lambdaArchive.endsWith("/target/lambda/mjers-lambda/bootstrap.zip"));
});
