<?php

declare(strict_types=1);

use Pam\Workflow\Context;
use Pam\Workflow\Definition;
use Pam\Workflow\Engine;
use Pam\Workflow\RetryPolicy;
use Pam\Workflow\Scheduler;
use Pam\Workflow\Step;
use Pam\Workflow\Store;

$database = $argv[1] ?? throw new InvalidArgumentException('database path is required');
$marker = $argv[2] ?? throw new InvalidArgumentException('marker path is required');
$compensations = [];
$activityKeys = [];

$retrying = new Definition('order.retry', 1, [
    new Step(
        'charge',
        static function (Context $context) use ($marker): string {
            if (!is_file($marker)) {
                file_put_contents($marker, 'attempted');
                throw new RuntimeException('transient gateway error');
            }
            return 'charged-' . $context->input['order'];
        },
        new RetryPolicy(maxAttempts: 2, initialDelaySeconds: 0),
    ),
    new Step(
        'receipt',
        static function (Context $context) use (&$activityKeys): string {
            $context->heartbeat();
            $activityKeys[] = $context->idempotencyKey();
            return 'receipt-' . $context->results['charge'];
        },
    ),
]);

$engine = (new Engine(new Store($database)))->register($retrying);
$waiting = $engine->start('order.retry', ['order' => 42], 'order-42');

$claimingStore = new Store($database);
$claimed = $claimingStore->claimDue('scheduler-a', limit: 1, leaseSeconds: 30);
$contended = $claimingStore->claimDue('scheduler-b', limit: 1, leaseSeconds: 30);
$wrongOwnerRejected = false;
$resumedEngine = (new Engine($claimingStore))->register($retrying);
$deduplicatedWhileLeased = $resumedEngine->start(
    'order.retry',
    ['order' => 999],
    'order-42',
);
try {
    $resumedEngine->runClaimed($waiting->id, 'scheduler-b');
} catch (LogicException) {
    $wrongOwnerRejected = true;
}
$completed = $resumedEngine->runClaimed($waiting->id, 'scheduler-a');
$claimingStore->releaseLease($waiting->id, 'scheduler-a');
$deduplicated = $resumedEngine->start('order.retry', ['order' => 999], 'order-42');

$compensating = new Definition('order.compensate', 1, [
    new Step(
        'reserve',
        static fn (): string => 'reservation-1',
        new RetryPolicy(maxAttempts: 1, initialDelaySeconds: 0),
        static function (Context $context, mixed $result) use (&$compensations): void {
            $compensations[] = [$context->instanceId, $result];
        },
    ),
    new Step(
        'ship',
        static fn (): never => throw new RuntimeException('shipping rejected'),
        new RetryPolicy(maxAttempts: 1, initialDelaySeconds: 0),
    ),
]);
$failedEngine = (new Engine(new Store($database)))->register($compensating);
$compensated = $failedEngine->start('order.compensate', ['order' => 77], 'order-77');

$leaseLossMarker = $marker . '.lease-loss';
$leaseLoss = new Definition('order.lease-loss', 1, [
    new Step(
        'deliver',
        static function (Context $context) use ($database, $leaseLossMarker): string {
            if (!is_file($leaseLossMarker)) {
                file_put_contents($leaseLossMarker, 'stolen');
                $thief = new Store($database);
                $stolen = $thief->claimDue(
                    'scheduler-thief',
                    limit: 1,
                    leaseSeconds: 30,
                    now: microtime(true) + 2,
                );
                if (count($stolen) !== 1) {
                    throw new RuntimeException('expected the expired lease to be recovered');
                }
                $context->heartbeat();
            }
            return $context->idempotencyKey();
        },
    ),
]);
$leaseLossStore = new Store($database);
$leaseLossInstance = $leaseLossStore->create(
    $leaseLoss,
    ['order' => 88],
    'lease-loss-88',
);
$leaseLossEngine = (new Engine($leaseLossStore))->register($leaseLoss);
$lostTick = (new Scheduler(
    $leaseLossStore,
    $leaseLossEngine,
    'scheduler-original',
    leaseSeconds: 1,
))->tick(limit: 1);
$stateAfterLeaseLoss = $leaseLossStore->find($leaseLossInstance->id);
$leaseLossStore->releaseLease($leaseLossInstance->id, 'scheduler-thief');
$recoveryTick = (new Scheduler(
    $leaseLossStore,
    $leaseLossEngine,
    'scheduler-recovered',
))->tick(limit: 1);
$stateAfterLeaseRecovery = $leaseLossStore->find($leaseLossInstance->id);

$scheduled = new Definition('order.scheduled', 1, [
    new Step('dispatch', static fn (Context $context): string => $context->idempotencyKey()),
]);
$schedulerStore = new Store($database);
$schedulerStore->create($scheduled, ['order' => 100], 'scheduled-100');
$recoverable = $schedulerStore->create($scheduled, ['order' => 101], 'scheduled-101');
$now = microtime(true);
$staleClaim = $schedulerStore->claimDue('scheduler-stale', limit: 2, leaseSeconds: 1, now: $now);
$recoveredClaim = $schedulerStore->claimDue(
    'scheduler-recovery',
    limit: 2,
    leaseSeconds: 30,
    now: $now + 2,
);
foreach ($recoveredClaim as $instance) {
    $schedulerStore->releaseLease($instance->id, 'scheduler-recovery');
}
$scheduledEngine = (new Engine($schedulerStore))->register($scheduled);
$tick = (new Scheduler($schedulerStore, $scheduledEngine, 'scheduler-main'))->tick(limit: 10);

$legacyDatabase = $database . '.legacy';
$legacy = new PDO("sqlite:{$legacyDatabase}", options: [
    PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
]);
$legacy->exec(
    'CREATE TABLE pam_workflow_instances (
        id TEXT PRIMARY KEY,
        definition TEXT NOT NULL,
        version INTEGER NOT NULL,
        state INTEGER NOT NULL,
        input_json TEXT NOT NULL,
        result_json TEXT,
        error TEXT,
        next_run_at REAL,
        idempotency_key TEXT NOT NULL,
        created_at REAL NOT NULL,
        updated_at REAL NOT NULL,
        UNIQUE (definition, idempotency_key)
    )',
);
new Store($legacyDatabase);
$legacyColumns = $legacy->query('PRAGMA table_info(pam_workflow_instances)')
    ->fetchAll(PDO::FETCH_COLUMN, 1);

echo json_encode([
    'waitingState' => $waiting->state->value,
    'completedState' => $completed->state->value,
    'completedResult' => $completed->result,
    'deduplicatedId' => $deduplicated->id,
    'deduplicatedWhileLeasedId' => $deduplicatedWhileLeased->id,
    'deduplicatedWhileLeasedState' => $deduplicatedWhileLeased->state->value,
    'originalId' => $waiting->id,
    'compensatedState' => $compensated->state->value,
    'compensations' => $compensations,
    'lostLeaseErrors' => count($lostTick->errors),
    'stateAfterLeaseLoss' => $stateAfterLeaseLoss->state->value,
    'recoveredLeaseCompleted' => $recoveryTick->completed,
    'stateAfterLeaseRecovery' => $stateAfterLeaseRecovery->state->value,
    'claimed' => count($claimed),
    'contended' => count($contended),
    'wrongOwnerRejected' => $wrongOwnerRejected,
    'activityKey' => $activityKeys[0] ?? null,
    'staleClaims' => count($staleClaim),
    'recoveredClaims' => count($recoveredClaim),
    'schedulerClaimed' => $tick->claimed,
    'schedulerCompleted' => $tick->completed,
    'schedulerErrors' => $tick->errors,
    'legacyLeaseColumns' => array_values(array_intersect(
        ['lease_owner', 'lease_expires_at'],
        $legacyColumns,
    )),
], JSON_THROW_ON_ERROR), PHP_EOL;
