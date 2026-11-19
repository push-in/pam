<?php

declare(strict_types=1);

namespace Nimbus\Jobs;

interface SerializableJob
{
    /** @return non-empty-string */
    public static function jobName(): string;

    /** @return array<string, bool|float|int|string|null|array<mixed>> */
    public function toJobPayload(): array;

    /**
     * @param array<string, mixed> $payload
     */
    public static function fromJobPayload(array $payload): self;
}
