// Execute the packaged arm64 bootstrap in the same AL2023 base as Lambda.
// A local Runtime API supplies one event that needs no AWS service calls.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const archive = resolve(process.argv[2] ?? join(root, 'target/lambda/mjers-lambda/bootstrap.zip'));
const directory = await mkdtemp(join(tmpdir(), 'mjers-lambda-'));
const container = `mjers-lambda-check-${process.pid}`;
let child;
let server;
let deadline;
try {
    execFileSync('unzip', ['-q', archive, '-d', directory]);
    const bootstrap = await readFile(join(directory, 'bootstrap'));
    assert.deepEqual([...bootstrap.subarray(0, 5)], [127, 69, 76, 70, 2]);
    assert.equal(bootstrap.readUInt16LE(18), 183, 'bootstrap must be Linux arm64');
    assert.ok((await stat(join(directory, 'bootstrap'))).mode & 0o111, 'bootstrap must be executable');
    const response = new Promise((resolveResponse, reject) => {
        let sent = false;
        server = createServer(async (request, response) => {
            if (request.method === 'GET' && request.url?.endsWith('/runtime/invocation/next')) {
                if (sent) return; // The container is stopped after the first response.
                sent = true;
                response.writeHead(200, {
                    'content-type': 'application/json',
                    'lambda-runtime-aws-request-id': 'packaging-check',
                    'lambda-runtime-deadline-ms': String(Date.now() + 30000),
                    'lambda-runtime-invoked-function-arn': 'arn:aws:lambda:us-east-1:000000000000:function:packaging-check',
                });
                response.end(JSON.stringify({ version: '2.0', rawPath: '/missing', requestContext: { http: { method: 'GET', sourceIp: '127.0.0.1' } } }));
            } else if (request.method === 'POST') {
                const chunks = [];
                for await (const chunk of request) chunks.push(chunk);
                response.writeHead(202);
                response.end();
                if (request.url?.endsWith('/response')) resolveResponse(JSON.parse(Buffer.concat(chunks).toString()));
                else reject(new Error('Lambda reported an initialization or invocation error.'));
            } else { response.writeHead(404); response.end(); }
        });
        server.listen(0, '0.0.0.0', () => {
            const port = server.address().port;
            child = spawn('docker', ['run', '--rm', '--name', container, '--platform', 'linux/arm64',
                ...(process.platform === 'linux' ? ['--add-host', 'host.docker.internal:host-gateway'] : []),
                '-v', `${directory}:/var/task:ro`,
                '-e', `AWS_LAMBDA_RUNTIME_API=host.docker.internal:${port}`,
                '-e', 'AWS_REGION=us-east-1', '-e', 'AWS_ACCESS_KEY_ID=local', '-e', 'AWS_SECRET_ACCESS_KEY=local',
                '-e', 'AWS_EC2_METADATA_DISABLED=true',
                '-e', 'AWS_LAMBDA_FUNCTION_NAME=packaging-check', '-e', 'AWS_LAMBDA_FUNCTION_VERSION=1',
                '-e', 'AWS_LAMBDA_FUNCTION_MEMORY_SIZE=512',
                '-e', 'TABLES_TABLE_NAME=unused', '-e', 'CONNECTIONS_TABLE_NAME=unused',
                '-e', 'WEBSOCKET_MANAGEMENT_ENDPOINT=http://127.0.0.1:1', '-e', 'SITE_ORIGIN=https://game.example',
                '--entrypoint', '/var/task/bootstrap', 'public.ecr.aws/lambda/provided:al2023'],
                { stdio: ['ignore', 'inherit', 'inherit'] });
            child.once('error', reject);
            child.once('exit', (code) => { if (code !== 0) reject(new Error(`Lambda container exited with ${code}`)); });
        });
        deadline = setTimeout(() => reject(new Error('Packaged Lambda runtime test timed out.')), 120000);
    });
    const result = await response;
    assert.equal(result.statusCode, 404);
    assert.equal(JSON.parse(result.body).code, 'invalid_room');
    console.log('Packaged Linux arm64 Lambda bootstrap executed successfully on AL2023.');
} finally {
    clearTimeout(deadline);
    try { execFileSync('docker', ['rm', '-f', container], { stdio: 'ignore' }); } catch { /* Already exited or never started. */ }
    child?.kill();
    server?.closeAllConnections();
    server?.close();
    await rm(directory, { recursive: true, force: true });
}
