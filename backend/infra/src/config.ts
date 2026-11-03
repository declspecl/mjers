import { fileURLToPath } from "node:url";

export interface DeploymentConfig {
    readonly stage: "dev" | "prod";
    readonly siteOrigin: string;
    readonly budgetEmail: string;
    readonly lambdaArchive: string;
    readonly env: { readonly account?: string; readonly region: string };
}

export function readDeploymentConfig(environment: NodeJS.ProcessEnv): DeploymentConfig {
    const stage = environment.DEPLOYMENT_STAGE;
    if (stage !== "dev" && stage !== "prod") throw new Error("Set DEPLOYMENT_STAGE to dev or prod.");
    const site = new URL(required(environment, "SITE_ORIGIN"));
    if (
        !["http:", "https:"].includes(site.protocol) ||
        site.pathname !== "/" ||
        site.search ||
        site.hash ||
        site.username ||
        site.password
    ) {
        throw new Error("SITE_ORIGIN must be an HTTP or HTTPS origin without a path or credentials.");
    }
    return {
        stage,
        siteOrigin: site.origin,
        budgetEmail: required(environment, "BUDGET_EMAIL"),
        lambdaArchive:
            environment.LAMBDA_ARCHIVE ?? fileURLToPath(new URL("../../../target/lambda/mjers-lambda/bootstrap.zip", import.meta.url)),
        env: { account: environment.CDK_DEFAULT_ACCOUNT, region: environment.AWS_REGION ?? "us-east-1" },
    };
}

function required(environment: NodeJS.ProcessEnv, name: string): string {
    const value = environment[name]?.trim();
    if (!value) throw new Error(`Set ${name}.`);
    return value;
}
