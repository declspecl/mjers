import { App } from "aws-cdk-lib";
import { Code } from "aws-cdk-lib/aws-lambda";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { readDeploymentConfig } from "./config.js";
import { MultiplayerStack } from "./stacks/multiplayer-stack.js";

const app = new App();
const config = readDeploymentConfig(process.env);
const archive = resolve(config.lambdaArchive);
if (!existsSync(archive)) throw new Error("Build the arm64 Lambda archive with backend/scripts/build-lambda.sh before synthesizing.");
new MultiplayerStack(app, `mjers-${config.stage}-backend`, {
    env: config.env,
    stage: config.stage,
    siteOrigins: config.siteOrigins,
    code: Code.fromAsset(archive),
});
