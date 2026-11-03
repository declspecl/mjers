import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

class Player {
    messages = [];
    waiters = [];

    constructor(socket) {
        this.socket = socket;
        socket.addEventListener('message', ({ data }) => {
            const message = JSON.parse(data);
            const waiter = this.waiters.shift();
            if (waiter) waiter(message);
            else this.messages.push(message);
        });
    }

    send(message) {
        this.socket.send(JSON.stringify(message));
    }

    async next(type, predicate = () => true) {
        for (;;) {
            const message = this.messages.length
                ? this.messages.shift()
                : await new Promise((resolve, reject) => {
                    const timer = setTimeout(() => reject(new Error(`Timed out waiting for ${type}`)), 5000);
                    this.waiters.push((message) => { clearTimeout(timer); resolve(message); });
                });
            if (message.type === type && predicate(message)) return message;
        }
    }
}

async function startServer(t, tableName) {
    const child = spawn('cargo', ['run', '--locked', '--quiet', '-p', 'mjers-dev-server'], {
        cwd: fileURLToPath(new URL('../../../', import.meta.url)),
        env: { ...process.env, MJERS_LISTEN: '127.0.0.1:0', MJERS_DYNAMODB_TABLE: tableName },
        stdio: ['ignore', 'ignore', 'pipe'],
    });
    const stop = async () => {
        if (child.exitCode !== null || child.signalCode !== null) return;
        const exited = once(child, 'exit');
        child.kill('SIGTERM');
        await exited;
    };
    t.after(stop);
    const address = await new Promise((resolve, reject) => {
        let output = '';
        const timeout = setTimeout(() => reject(new Error(`Server startup timed out: ${output}`)), 90000);
        child.stderr.on('data', (data) => {
            output += data;
            const match = output.match(/listening on (127\.0\.0\.1:\d+)/);
            if (match) { clearTimeout(timeout); resolve(match[1]); }
        });
        child.once('error', (error) => { clearTimeout(timeout); reject(error); });
        child.once('exit', () => { clearTimeout(timeout); reject(new Error(output)); });
    });
    return { address, stop };
}

test('DynamoDB rooms survive restarts and support a full two-human hand', { timeout: 120000 }, async (t) => {
    const tableName = process.env.MJERS_DYNAMODB_TABLE ?? 'mjers-test-sockets';
    let server = await startServer(t, tableName);
    const post = async (path, body, status = 200) => {
        const response = await fetch(`http://${server.address}${path}`, {
            method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
        });
        assert.equal(response.status, status);
        return response.json();
    };
    const connect = async (joined) => {
        const socket = new WebSocket(`ws://${server.address}/ws?table=${joined.table}&token=${joined.seat_token}`);
        const player = new Player(socket);
        t.after(() => socket.close());
        await once(socket, 'open');
        return player;
    };
    const host = await post('/tables', { name: 'Host', password: 'family' });
    assert.equal(host.seat, 0);
    assert.match(host.table, /^[A-Z2-9]{6}$/);
    assert.equal(host.join_path, `/?room=${host.table}`);
    let a = await connect(host);
    let state = await a.next('state');
    assert.equal(state.human_players, 2);
    assert.equal(state.password_protected, true);
    assert.equal(state.seats[1].kind, 'open');
    assert.equal(state.seats[2].kind, 'bot');
    a.send({ type: 'start_hand' });
    assert.equal((await a.next('error')).code, 'not_ready');
    const denied = await post(`/tables/${host.table}/join`, { name: 'Guest', password: 'wrong' }, 403);
    assert.equal(denied.code, 'bad_password');
    const guest = await post(`/tables/${host.table}/join`, { name: '家人', password: 'family' });
    assert.equal(guest.seat, 1);
    await post(`/tables/${host.table}/join`, { name: 'Extra', password: 'family' }, 400);
    const old = await connect(guest);
    await old.next('state');
    const replaced = once(old.socket, 'close');
    let b = await connect(guest);
    await replaced;
    assert.equal((await b.next('state')).seats[1].name, '家人');
    b.send({ type: 'start_hand' });
    assert.equal((await b.next('error')).code, 'not_host');
    a.send({ type: 'start_hand' });
    state = await a.next('state', (s) => s.hand_number === 1);
    // Persist an action before killing the whole HTTP/WebSocket process.
    a.send({ type: 'act', action: state.observation.legal_actions[0], hand_number: 1, steps_seen: state.steps });
    state = await a.next('state', (s) => s.steps > state.steps);
    const beforeRestart = state;
    await server.stop();
    server = await startServer(t, tableName);
    a = await connect(host);
    b = await connect(guest);
    state = await a.next('state', (s) => s.seats[1].connected);
    assert.equal(state.version, beforeRestart.version);
    assert.equal(state.steps, beforeRestart.steps);
    assert.deepEqual(state.observation, beforeRestart.observation);
    assert.deepEqual(state.scores, beforeRestart.scores);
    let firstCommand;
    for (let turn = 0; turn < 1000; turn++) {
        const asked = state.observation.to_act;
        if (asked.length === 0) break;
        const seat = asked[0];
        assert.ok(seat === 0 || seat === 1);
        const player = seat === 0 ? a : b;
        const own = seat === 0 ? state : await b.next('state', (s) => s.hand_number === 1 && s.steps === state.steps);
        assert.equal(own.observation.seat, seat);
        assert.ok(own.observation.players.every((p) => !('concealed' in p)));
        const legal = own.observation.legal_actions;
        const action = legal.find((a) => a === 'self_draw_win' || a === 'win') ?? legal[0];
        const command = { type: 'act', action, hand_number: 1, steps_seen: state.steps };
        firstCommand ??= command;
        player.send(command);
        state = await a.next('state', (s) => s.steps > state.steps);
    }
    assert.deepEqual(state.observation.to_act, []);
    const over = await a.next('hand_over');
    assert.equal(over.revealed.length, 4);
    assert.equal(over.settlement.total.reduce((a, b) => a + b, 0), 0);
    assert.deepEqual(state.scores, over.settlement.total);
    a.send(firstCommand);
    assert.equal((await a.next('error')).code, 'stale');
    a.send({ type: 'next_hand', hand_number: 1 });
    const next = await a.next('state', (s) => s.hand_number === 2);
    assert.equal(next.observation.dealer, 1);
    const restored = await connect(host);
    const resumed = await restored.next('state');
    assert.equal(resumed.hand_number, 2);
    assert.equal(resumed.steps, next.steps);
    restored.send({ type: 'ping' });
    await restored.next('pong');
});


test('two server processes compete for the last human seat through DynamoDB', { timeout: 120000 }, async (t) => {
    const tableName = process.env.MJERS_DYNAMODB_TABLE ?? 'mjers-test-sockets';
    const a = await startServer(t, tableName);
    const b = await startServer(t, tableName);
    const post = (server, path, body) => fetch(`http://${server.address}${path}`, {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
    });
    const response = await post(a, '/tables', { name: 'Host' });
    assert.equal(response.status, 200);
    const room = await response.json();
    const attempts = await Promise.all([
        post(a, `/tables/${room.table}/join`, { name: 'A' }),
        post(b, `/tables/${room.table}/join`, { name: 'B' }),
    ]);
    assert.deepEqual(attempts.map((response) => response.status).sort(), [200, 400]);
    const results = await Promise.all(attempts.map((response) => response.json()));
    assert.equal(results.find((result) => result.code)?.code, 'table_full');
    assert.equal(results.find((result) => result.seat)?.seat, 1);
});
