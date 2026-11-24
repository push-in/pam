<?php

declare(strict_types=1);

namespace Nimbus\Events;

/**
 * Marks an event listener that runs on the job queue instead of during `dispatch()`.
 *
 * The dispatcher pushes a `CallQueuedListener` job to the bound `Jobs\JobDispatcher`; a
 * worker resolves the listener from the container and calls it with the event. Optional
 * public properties `int $tries` (default 3) and `int $delay` (seconds), and an optional
 * `shouldQueue(object $event): bool` method, tune or skip the job.
 *
 * The event must survive the queue: implement `Jobs\SerializableJob`, or keep only
 * scalar, array and null public properties that the constructor accepts by name.
 */
interface ShouldQueue
{
}
