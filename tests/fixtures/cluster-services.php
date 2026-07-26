<?php

declare(strict_types=1);

use Pam\Cluster\CircuitState;
use Pam\Cluster\InMemoryCoordinator;
use Pam\Cluster\Lease;
use Pam\Cluster\NodeState;

$now = 1000.0;
$cluster = new InMemoryCoordinator(static function () use (&$now): float {
    return $now;
});

$cluster->heartbeat(
    'node-a',
    'https://10.0.0.1:443',
    NodeState::Ready,
    ['region' => 'sa-east-1'],
    5,
);
$cluster->heartbeat('node-b', 'https://10.0.0.2:443', NodeState::Draining, ttlSeconds: 10);
$initialNodes = $cluster->discover();
$now += 6;
$remainingNodes = $cluster->discover();

$first = $cluster->acquire('orders', 5);
assert($first !== null);
$contended = $cluster->acquire('orders', 5);
$renewed = $cluster->renew($first, 10);
assert($renewed !== null);
$forgedRelease = $cluster->release(new Lease(
    $renewed->name,
    'forged',
    $renewed->fencingToken,
    $renewed->expiresAt,
));
$now += 11;
$second = $cluster->acquire('orders', 5);
assert($second !== null);

$singleton = $cluster->singleton('billing', 60, 120);
$duplicateSingleton = $cluster->singleton('billing', 60, 120);

$rateOne = $cluster->rateLimit('user-42', 2, 10);
$rateTwo = $cluster->rateLimit('user-42', 2, 10);
$rateThree = $cluster->rateLimit('user-42', 2, 10);
$now += 10;
$rateReset = $cluster->rateLimit('user-42', 2, 10);

$closed = $cluster->circuitPermit('payments', 2, 5);
$failureOne = $cluster->circuitFailure('payments', 2, 5);
$failureTwo = $cluster->circuitFailure('payments', 2, 5);
$blocked = $cluster->circuitPermit('payments', 2, 5);
$now += 6;
$probe = $cluster->circuitPermit('payments', 2, 5);
$secondProbe = $cluster->circuitPermit('payments', 2, 5);
$cluster->circuitSuccess('payments');
$recovered = $cluster->circuitPermit('payments', 2, 5);

$cluster->enqueue('mail', 'a', 2);
$cluster->enqueue('mail', 'b', 2);
$queueLength = $cluster->enqueue('mail', 'c', 2);
$queue = [$cluster->dequeue('mail'), $cluster->dequeue('mail'), $cluster->dequeue('mail')];

$cluster->presenceJoin('room-1', 'alice', 5);
$cluster->presenceJoin('room-1', 'bob', 10);
$presenceInitial = $cluster->presenceMembers('room-1');
$now += 6;
$presenceAfterExpiry = $cluster->presenceMembers('room-1');
$cluster->presenceLeave('room-1', 'bob');

echo json_encode([
    'initialNodeStates' => array_map(
        static fn ($node): int => $node->state->value,
        $initialNodes,
    ),
    'remainingNode' => $remainingNodes[0]->id,
    'contended' => $contended === null,
    'renewedFence' => $renewed->fencingToken,
    'forgedRelease' => $forgedRelease,
    'newFence' => $second->fencingToken,
    'singleton' => $singleton !== null && $duplicateSingleton === null,
    'rates' => [
        $rateOne->allowed,
        $rateTwo->allowed,
        $rateThree->allowed,
        $rateReset->allowed,
    ],
    'circuits' => [
        $closed->state->value,
        $failureOne->value,
        $failureTwo->value,
        $blocked->allowed,
        $probe->state->value,
        $secondProbe->allowed,
        $recovered->state->value,
    ],
    'queueLength' => $queueLength,
    'queue' => $queue,
    'presenceInitial' => $presenceInitial,
    'presenceAfterExpiry' => $presenceAfterExpiry,
    'presenceAfterLeave' => $cluster->presenceMembers('room-1'),
], JSON_THROW_ON_ERROR);
