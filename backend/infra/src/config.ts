import { fileURLToPath } from "node:url";

export interface DeploymentConfig {
    readonly stage: "dev" | "prod";
    readonly siteOrigins: readonly string[];
    readonly lambdaArchive: string;
    readonly env: { readonly account?: string; readonly region: string };
}

export function readDeploymentConfig(environment: NodeJS.ProcessEnv): DeploymentConfig {
    const stage = environment.DEPLOYMENT_STAGE;
    if (stage !== "dev" && stage !== "prod") throw new Error("Set DEPLOYMENT_STAGE to dev or prod.");
    const siteOrigins = required(environment, "SITE_ORIGINS")
        .split(",")
        .map((value) => {
            const site = new URL(value.trim());
            if (
                !["http:", "https:"].includes(site.protocol) ||
                site.pathname !== "/" ||
                site.search ||
                site.hash ||
                site.username ||
                site.password
            ) {
                throw new Error("SITE_ORIGINS must contain HTTP or HTTPS origins without paths or credentials.");
            }
            return site.origin;
        });
    return {
        stage,
        siteOrigins,
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
