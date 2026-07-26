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
use Pam\Workflow\Scheduler;
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

$store = new Store(storage_path('pam/workflows.sqlite'));
$engine = (new Engine($store))->register($orders);

$instance = $engine->start(
    'order.fulfill',
    ['orderId' => 42],
    idempotencyKey: 'order-42',
);

$scheduler = new Scheduler(
    $store,
    $engine,
    owner: gethostname() . ':' . getmypid(),
    leaseSeconds: 30,
);
$tick = $scheduler->tick(limit: 100);
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
absolute `next_run_at`, then returns a waiting instance. `Scheduler::tick()`
atomically claims due work, executes it under a bounded lease and releases the
claim. Completed step results are reused and not executed again.

Multiple scheduler processes may share the same local database. `BEGIN
IMMEDIATE` serializes the claim transaction, and the owner plus expiry columns
prevent two healthy schedulers from receiving the same instance. Expired
leases make interrupted pending, running, waiting or compensating instances
claimable again. Activities that may run longer than the lease call
`$context->heartbeat()` at safe checkpoints.

PAM checks the lease again after every activity and compensation before
committing its result. A worker that lost ownership fails closed and leaves the
step resumable instead of recording stale success or failure. Duplicate
`start()` calls made while another scheduler owns the instance return the
persisted instance without competing with it.

After a terminal activity failure, completed steps with compensation handlers
run in reverse definition order. Compensation state and the original failure are
persisted. Interrupted compensation resumes from its persisted step states. A
compensation failure leaves the instance failed with both errors.

Every instance transition is also published as diagnostics event kind `9`.

## Delivery contract

The store guarantees durable orchestration history, idempotent workflow
creation and exclusive execution while a lease remains valid. It cannot make
arbitrary external side effects exactly-once. Activity and compensation
implementations must send `$context->idempotencyKey()` to payment, mail, queue
and other external systems. A process can stop after the side effect and before
the local commit, so receivers must deduplicate that stable key.

Definitions are code and are registered per process. Keep previous definition
versions available while instances using them can still resume. Changing the
step order or names under an existing version is rejected when persisted history
is loaded.

SQLite WAL is the embedded single-host store and supports competing scheduler
processes on that host. Do not place it on an unsupported network filesystem.
A future SQL repository is still required for geographically distributed
workflow storage; the lease and scheduler contract no longer depends on manual
`run()` calls.
