# Distributed cluster services

The Rust supervisor handles workers and zero-downtime generations on one host.
`Pam\Cluster\RedisCoordinator` supplies the shared coordination plane required
when an application spans hosts or regions.

It provides:

- expiring service discovery with explicit ready/draining/offline states;
- owner-token locks with monotonically increasing fencing tokens;
- singleton cron buckets;
- atomic fixed-window rate limits;
- closed/open/half-open circuit breakers with one half-open probe;
- bounded FIFO queues;
- expiring WebSocket/user presence.

All state-like values use sequential integer enums on the wire and in Redis.

## Secure connection

```php
use Pam\Cluster\RedisCoordinator;

$cluster = new RedisCoordinator(
    host: $_ENV['PAM_REDIS_HOST'],
    port: 6380,
    prefix: 'production:checkout:pam:',
    username: $_ENV['PAM_REDIS_USERNAME'],
    password: $_ENV['PAM_REDIS_PASSWORD'],
    tls: true,
    caFile: '/run/secrets/redis-ca.pem',
    certificateFile: '/run/secrets/pam-node.pem',
    privateKeyFile: '/run/secrets/pam-node-key.pem',
);
```

TLS always verifies the peer name and certificate. Providing a client
certificate and private key enables mTLS. Keep credentials in runtime secrets,
not source code or the bundle.

## Discovery and draining

```php
use Pam\Cluster\NodeState;

$cluster->heartbeat(
    nodeId: $_ENV['PAM_NODE_ID'],
    endpoint: 'https://10.0.1.8:443',
    state: NodeState::Ready,
    metadata: ['region' => 'sa-east-1', 'generation' => 42],
    ttlSeconds: 15,
);

$ready = array_filter(
    $cluster->discover(),
    static fn ($node): bool => $node->state === NodeState::Ready,
);
```

Send heartbeats more frequently than the TTL. Before shutdown, publish
`NodeState::Draining`, stop accepting new work, wait for active work to finish,
and then allow the heartbeat to expire.

## Fenced locks and singleton cron

```php
$lease = $cluster->acquire('invoice:42', ttlSeconds: 30);
if ($lease === null) {
    return;
}

try {
    // Send fencingToken with the write. The storage boundary must reject a token
    // lower than the latest one it has already observed.
    updateInvoice(42, $lease->fencingToken);
    $lease = $cluster->renew($lease, ttlSeconds: 30)
        ?? throw new RuntimeException('Lost invoice lease.');
} finally {
    $cluster->release($lease);
}

if (($cron = $cluster->singleton('settlements', intervalSeconds: 60)) !== null) {
    try {
        settleInvoices();
    } finally {
        $cluster->release($cron);
    }
}
```

The random owner token prevents another owner from renewing or releasing a
lease. The fencing token protects external storage from a paused former owner
that wakes after its lease expired.

## Limits and circuit breakers

```php
$rate = $cluster->rateLimit("tenant:{$tenantId}", limit: 500, windowSeconds: 1);
if (!$rate->allowed) {
    throw new TooManyRequests($rate->resetsAt);
}

$permit = $cluster->circuitPermit('payments', failureThreshold: 5, openSeconds: 30);
if (!$permit->allowed) {
    throw new ServiceUnavailable();
}

try {
    charge();
    $cluster->circuitSuccess('payments');
} catch (Throwable $error) {
    $cluster->circuitFailure('payments', failureThreshold: 5, openSeconds: 30);
    throw $error;
}
```

Redis scripts make each decision atomic across nodes. The half-open state admits
only one probe until it reports success or failure.

## Queues, pub/sub and presence

```php
$cluster->enqueue('mail', json_encode($job, JSON_THROW_ON_ERROR), capacity: 100_000);
$payload = $cluster->dequeue('mail');

$cluster->presenceJoin('room:orders', (string) $userId, ttlSeconds: 30);
$members = $cluster->presenceMembers('room:orders');
$cluster->presenceLeave('room:orders', (string) $userId);
```

The embedded queue is intentionally bounded and FIFO. Use Laravel queues or a
durable broker for acknowledgement, delayed delivery and dead-letter policies.
For multi-node live broadcast, pair the coordinator with PAM's Redis Streams or
TLS-enabled NATS WebSocket adapter.

`InMemoryCoordinator` implements the same contract for unit tests. It accepts an
injectable clock, making expiry, fencing, rate windows and breaker recovery
deterministic.
