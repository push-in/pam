<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\Broadcasting\BroadcastFake;
use Nimbus\Broadcasting\BroadcastManager;

/**
 * Test facade: `Broadcast::fake()` swaps the BroadcastManager's driver for a recording
 * BroadcastFake (binding a manager when the application has none).
 */
final class Broadcast
{
    private function __construct()
    {
    }

    public static function fake(): BroadcastFake
    {
        $fake = new BroadcastFake();
        $container = Fakes::app()->container();
        $manager = $container->has(BroadcastManager::class) ? $container->get(BroadcastManager::class) : null;
        if ($manager instanceof BroadcastManager) {
            $manager->swap($fake);
        } else {
            $container->instance(BroadcastManager::class, new BroadcastManager($fake));
        }
        return Fakes::remember($fake);
    }

    /** @param (\Closure(array<array-key, mixed>): bool)|null $callback */
    public static function assertBroadcast(string $event, ?string $channel = null, ?\Closure $callback = null): void
    {
        Fakes::get(BroadcastFake::class, 'Broadcast')->assertBroadcast($event, $channel, $callback);
    }

    public static function assertNotBroadcast(string $event, ?string $channel = null): void
    {
        Fakes::get(BroadcastFake::class, 'Broadcast')->assertNotBroadcast($event, $channel);
    }

    public static function assertNothingBroadcast(): void
    {
        Fakes::get(BroadcastFake::class, 'Broadcast')->assertNothingBroadcast();
    }
}
