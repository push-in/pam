# Server transport plugins

PAM packages can add queues, pub/sub brokers, ordered streams, and RPC systems
without importing the Rust runtime implementation. The stable contract lives in
`pushinbr/pam-contracts` under `Pam\Contracts\Transport`.

## Provider contract

Each provider returns a `TransportDescriptor` with a bounded lowercase ID,
protocol version, payload ceiling, batch ceiling, kind, and unique capabilities.
Discriminators are sequential integer enums:

| Contract | Values |
| --- | --- |
| Kind | Queue `1`, PubSub `2`, Stream `3`, RPC `4` |
| Capability | Publish `1`, Consume `2`, DelayedRetry `3`, DeadLetter `4`, OrderedDelivery `5` |
| Disposition | Acknowledge `1`, Retry `2`, Reject `3` |

```php
use Pam\Contracts\Transport\MessageResult;
use Pam\Contracts\Transport\TransportCapability;
use Pam\Contracts\Transport\TransportContext;
use Pam\Contracts\Transport\TransportDescriptor;
use Pam\Contracts\Transport\TransportKind;
use Pam\Contracts\Transport\TransportMessage;
use Pam\Contracts\Transport\TransportProviderInterface;
use Pam\Contracts\Transport\TransportWorker;

$descriptor = new TransportDescriptor(
    id: 'acme.orders',
    kind: TransportKind::Queue,
    capabilities: [
        TransportCapability::Consume,
        TransportCapability::DelayedRetry,
    ],
    maxPayloadBytes: 1_048_576,
    maxBatchSize: 100,
);

$processed = TransportWorker::run(
    provider: $provider,
    handler: static function (TransportMessage $message): MessageResult {
        processOrder($message->payload);
        return MessageResult::acknowledge();
    },
    context: new TransportContext(
        workerId: 'orders-1',
        cancelled: static fn (): bool => shutdownRequested(),
        observe: static fn (int $eventCode, array $attributes) => recordMetric(
            $eventCode,
            $attributes,
        ),
    ),
);
```

`TransportWorker` refuses oversized payloads, providers that exceed the requested
batch, non-message yields, unbounded waits, and handlers that return an invalid
result. It always calls `stop()` through `finally`. Handler failures become retry
when the descriptor advertises delayed retry; otherwise they are rejected. The
observation stream uses append-only integer event codes and never includes message
payloads, IDs, headers, or exception messages.

## Application registration

`Pam\App` implements the additive `TransportApplicationInterface`. A package
service provider can register an adapter without breaking alternative HTTP-only
implementations:

```php
use Pam\Contracts\Transport\TransportApplicationInterface;

if ($application instanceof TransportApplicationInterface) {
    $application->transport(new AcmeOrdersProvider($client));
}
```

IDs are unique and registration freezes with the rest of the application.
Credentials remain constructor/runtime secrets and must never appear in the
descriptor, diagnostics attributes, generated caches, or source control.

## Conformance requirements

A transport package is compatible only when its tests prove:

1. protocol, kind, capability, payload, and batch descriptor validation;
2. bounded receive behavior and exact `TransportMessage` values;
3. acknowledgement, retry, reject, cancellation, and unconditional stop paths;
4. payload-free observations using the shared event enum;
5. recovery after broker disconnect without replaying acknowledged messages;
6. no credentials in exceptions, logs, descriptors, or fixtures.
