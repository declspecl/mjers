// Uploads dist/ to the Cloudflare Pages project in wrangler.jsonc.
// CI sets CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID. Locally, the login
// from the `cf` CLI (`cf auth login`) supplies both, so wrangler needs no login of its own.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import process from "node:process";

const env = { ...process.env };

if (!env.CLOUDFLARE_API_TOKEN) {
    let whoami;
    try {
        whoami = JSON.parse(execFileSync("cf", ["auth", "whoami"], { encoding: "utf8" }));
    } catch (error) {
        throw new Error("Could not read the cf CLI login. Run `cf auth login`, or set CLOUDFLARE_API_TOKEN.", { cause: error });
    }
    if (!whoami.authenticated || !whoami.tokenValid) {
        throw new Error("The cf CLI is not logged in or its token expired. Run `cf auth login`, then deploy again.");
    }
    const configPath = /^OAuth token from (.+)$/.exec(whoami.authSource ?? "")?.[1];
    if (!configPath) {
        throw new Error(`The cf CLI login is not an OAuth token (${whoami.authSource}). Set CLOUDFLARE_API_TOKEN instead.`);
    }
    env.CLOUDFLARE_API_TOKEN = JSON.parse(readFileSync(configPath, "utf8")).oauth_token;
    if (!env.CLOUDFLARE_ACCOUNT_ID) {
        if (whoami.accounts?.length !== 1) {
            throw new Error("The cf CLI login can see more than one account. Set CLOUDFLARE_ACCOUNT_ID.");
        }
        env.CLOUDFLARE_ACCOUNT_ID = whoami.accounts[0].id;
    }
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
const branch = env.GITHUB_REF_NAME ?? git("rev-parse", "--abbrev-ref", "HEAD");
const commit = env.GITHUB_SHA ?? git("rev-parse", "HEAD");
const dirty = git("status", "--porcelain", "--", ".") !== "";

execFileSync("pnpm", ["exec", "wrangler", "pages", "deploy", "--branch", branch, "--commit-hash", commit, `--commit-dirty=${dirty}`], {
    env,
    stdio: "inherit",
});
