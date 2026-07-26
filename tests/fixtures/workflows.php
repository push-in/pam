<?php

declare(strict_types=1);

use Pam\Workflow\Context;
use Pam\Workflow\Definition;
use Pam\Workflow\Engine;
use Pam\Workflow\RetryPolicy;
use Pam\Workflow\Step;
use Pam\Workflow\Store;

$database = $argv[1] ?? throw new InvalidArgumentException('database path is required');
$marker = $argv[2] ?? throw new InvalidArgumentException('marker path is required');
$compensations = [];

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
    new Step('receipt', static fn (Context $context): string => 'receipt-' . $context->results['charge']),
]);

$engine = (new Engine(new Store($database)))->register($retrying);
$waiting = $engine->start('order.retry', ['order' => 42], 'order-42');

// Recreate both persistence and execution objects. Only the SQLite history is
// carried across this boundary.
$resumedEngine = (new Engine(new Store($database)))->register($retrying);
$completed = $resumedEngine->run($waiting->id);
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

echo json_encode([
    'waitingState' => $waiting->state->value,
    'completedState' => $completed->state->value,
    'completedResult' => $completed->result,
    'deduplicatedId' => $deduplicated->id,
    'originalId' => $waiting->id,
    'compensatedState' => $compensated->state->value,
    'compensations' => $compensations,
], JSON_THROW_ON_ERROR), PHP_EOL;
