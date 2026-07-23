<?php

declare(strict_types=1);

use Pam\WS\InMemoryAdapter;
use Pam\WS\NatsAdapter;

use function Pam\Async\delay;

$memory = new InMemoryAdapter(capacity: 2);
$memory->publish('room', 'first');
$memory->publish('room', 'second');
$memory->publish('room', 'third');
$memoryMessages = iterator_to_array($memory->poll(), false);

$nats = new NatsAdapter(port: (int) $argv[1], maxRetries: 1);
$nats->publish('broadcast', '{"value":"fragmented"}');
$natsMessages = [];
$deadline = microtime(true) + 2.0;
while ($natsMessages === [] && microtime(true) < $deadline) {
    $natsMessages = iterator_to_array($nats->poll(), false);
    if ($natsMessages === []) {
        delay(0.001);
    }
}

echo json_encode([
    'memory' => $memoryMessages,
    'nats' => $natsMessages,
], JSON_THROW_ON_ERROR), PHP_EOL;
