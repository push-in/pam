# Durable workflows

PAM workflows persist orchestration history in SQLite WAL instead of keeping it
only in worker memory. A process can stop after a retry is scheduled and another
engine instance can resume the same workflow from its stored step history.

## Define and run

```php
use Pam\Workflow\Context;
use Pam\Workflow\Definition;
use Pam\Workflow\Engine;
use Pam\Workflow\RetryPolicy;
use Pam\Workflow\Step;
use Pam\Workflow\Store;

$orders = new Definition('order.fulfill', version: 1, steps: [
    new Step(
        'charge',
        static fn (Context $context) => charge($context->input['orderId']),
        new RetryPolicy(maxAttempts: 5, initialDelaySeconds: 1, multiplier: 2),
        static fn (Context $context, mixed $charge) => refund($charge),
    ),
    new Step(
        'ship',
        static fn (Context $context) => ship($context->results['charge']),
        new RetryPolicy(maxAttempts: 3, initialDelaySeconds: 5),
    ),
]);

$engine = (new Engine(new Store(storage_path('pam/workflows.sqlite'))))
    ->register($orders);

$instance = $engine->start(
    'order.fulfill',
    ['orderId' => 42],
    idempotencyKey: 'order-42',
);
```

Names are stable identifiers and versions are positive integers. Calling
`start()` again with the same definition and idempotency key returns the existing
instance; it never creates a second execution.

## State model

Instance states are sequential integer enums:

| Value | State |
| ---: | --- |
| `1` | pending |
| `2` | running |
| `3` | waiting |
| `4` | completed |
| `5` | failed |
| `6` | compensating |
| `7` | compensated |

Step states use the same integer contract for pending, running, waiting,
completed, failed and compensated. No string status is stored in the database.

When an activity fails before `maxAttempts`, PAM persists the attempt, error and
absolute `next_run_at`, then returns a waiting instance. A scheduler or queue
worker calls `$engine->run($instanceId)` after the deadline. Completed step
results are reused and not executed again.

After a terminal activity failure, completed steps with compensation handlers
run in reverse definition order. Compensation state and the original failure are
persisted. A compensation failure leaves the instance failed with both errors.

Every instance transition is also published as diagnostics event kind `9`.

## Delivery contract

The store guarantees durable orchestration history and idempotent workflow
creation. It cannot make arbitrary external side effects exactly-once. Activity
implementations must send the workflow instance/step identity as the idempotency
key to payment, mail, queue and other external systems.

Definitions are code and are registered per process. Keep previous definition
versions available while instances using them can still resume. Changing the
step order or names under an existing version is rejected when persisted history
is loaded.

SQLite is the embedded single-node store. A distributed database repository,
leased claim loop and multi-node scheduler remain required before calling this a
distributed workflow service.
