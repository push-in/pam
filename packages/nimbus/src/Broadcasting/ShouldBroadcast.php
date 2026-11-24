<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/**
 * An event that is pushed to WebSocket channels when it is dispatched. It is queued as a
 * `BroadcastEvent` job when a `Jobs\JobDispatcher` is bound; implement
 * `ShouldBroadcastNow` to broadcast during `dispatch()`.
 *
 * Optional methods: `broadcastAs(): string` (the event name, default the class name),
 * `broadcastWith(): array` (the payload, default the public properties) and
 * `broadcastWhen(): bool`.
 */
interface ShouldBroadcast
{
    /** @return Channel|string|list<Channel|string> */
    public function broadcastOn(): Channel|string|array;
}
