<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

/** One recorded event of the debug panel, grouped by the request token it happened in. */
final readonly class DebugEntry
{
    /** @param array<string, mixed> $data */
    public function __construct(
        public string $id,
        public DebugEntryType $type,
        public ?string $requestToken,
        public float $recordedAt,
        public array $data,
    ) {
    }

    /** @return array{id: string, type: string, request: ?string, recorded_at: float, data: array<string, mixed>} */
    public function toArray(): array
    {
        return [
            'id' => $this->id,
            'type' => $this->type->value,
            'request' => $this->requestToken,
            'recorded_at' => $this->recordedAt,
            'data' => $this->data,
        ];
    }
}
