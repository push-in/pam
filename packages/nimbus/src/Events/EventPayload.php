<?php

declare(strict_types=1);

namespace Nimbus\Events;

use Nimbus\Jobs\SerializableJob;

/**
 * @internal Converts events to the JSON data of queued listener jobs and back.
 */
final class EventPayload
{
    private function __construct()
    {
    }

    /** @return array<string, mixed> */
    public static function encode(object $event): array
    {
        if ($event instanceof SerializableJob) {
            return $event->toJobPayload();
        }
        $data = get_object_vars($event);
        array_walk_recursive($data, static function (mixed $value) use ($event): void {
            if ($value !== null && !is_scalar($value)) {
                throw new \LogicException(
                    $event::class . ' cannot be queued: implement Nimbus\\Jobs\\SerializableJob or keep only scalar, array and null public properties.',
                );
            }
        });
        return $data;
    }

    /**
     * @param class-string $class
     * @param array<string, mixed> $data
     */
    public static function decode(string $class, array $data): object
    {
        if (is_subclass_of($class, SerializableJob::class)) {
            return $class::fromJobPayload($data);
        }
        return new $class(...$data);
    }
}
