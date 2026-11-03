// Run only after deployment: node backend/scripts/smoke-aws.mjs HTTPS_URL WSS_URL
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

const [http, sockets] = process.argv.slice(2);
if (!http || !sockets || new URL(http).protocol !== 'https:' || new URL(sockets).protocol !== 'wss:') {
    throw new Error('Provide the deployed HTTPS and WSS endpoints from CDK outputs.');
}
const players = [];
class Player {
    queue = [];
    waiters = [];
    constructor(joined) {
        const url = new URL(sockets);
        url.searchParams.set('table', joined.table);
        url.searchParams.set('token', joined.seat_token);
        this.socket = new WebSocket(url);
        this.open = new Promise((resolve, reject) => {
            this.socket.addEventListener('open', resolve, { once: true });
            this.socket.addEventListener('error', () => reject(new Error('WebSocket connection failed.')), { once: true });
        });
        this.socket.addEventListener('message', ({ data }) => {
            const value = JSON.parse(data);
            const waiter = this.waiters.shift();
            if (waiter) waiter(value); else this.queue.push(value);
        });
        players.push(this);
    }
    send(value) { this.socket.send(JSON.stringify(value)); }
    async next(type) {
        for (;;) {
            const value = this.queue.length ? this.queue.shift() : await new Promise((resolve, reject) => {
                const timer = setTimeout(() => reject(new Error(`Timed out waiting for ${type}.`)), 15000);
                this.waiters.push((value) => { clearTimeout(timer); resolve(value); });
            });
            if (value.type === 'error') throw new Error(`Backend rejected the smoke test: ${value.code}.`);
            if (value.type === type) return value;
        }
    }
    async sync() {
        await this.open;
        this.send({ type: 'sync' });
        return this.next('state');
    }
}
const post = async (path, body, expected = 200) => {
    const response = await fetch(`${http.replace(/\/$/, '')}${path}`, {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body), signal: AbortSignal.timeout(30000),
    });
    assert.equal(response.status, expected);
    return response.json();
};
try {
    const password = randomUUID();
    const host = await post('/tables', { name: 'Backend smoke host', human_players: 2, password });
    await post(`/tables/${host.table}/join`, { name: 'Backend smoke guest', password: 'wrong' }, 403);
    const guest = await post(`/tables/${host.table}/join`, { name: 'Backend smoke guest', password });
    const a = new Player(host);
    await a.sync();
    const b = new Player(guest);
    await b.sync();
    a.send({ type: 'start_hand' });
    let state;
    do { state = await a.next('state'); } while (state.hand_number !== 1);
    a.send({ type: 'autoplay', enabled: true, hand_number: 1, steps_seen: state.steps });
    // Wait for the host's autoplay acknowledgement before letting the guest act.
    do { state = await b.next('state'); } while (!state.seats[0].autoplay);
    b.send({ type: 'autoplay', enabled: true, hand_number: 1, steps_seen: state.steps });
    const over = await a.next('hand_over');
    assert.equal(over.revealed.length, 4);
    assert.equal(over.settlement.total.reduce((a, b) => a + b, 0), 0);
    b.socket.close();
    const restored = new Player(guest);
    state = await restored.sync();
    assert.equal(state.hand_number, 1);
    assert.deepEqual(state.scores, over.settlement.total);
    assert.equal((await restored.next('hand_over')).hand_number, 1);
    a.send({ type: 'next_hand', hand_number: 1 });
    do { state = await restored.next('state'); } while (state.hand_number !== 2);
    assert.equal(state.observation.dealer, 1);
    console.log('AWS backend smoke test passed: create, password, join, sockets, full hand, reconnect and next hand.');
} finally {
    for (const player of players) player.socket.close();
}
