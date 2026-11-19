<?php

declare(strict_types=1);

namespace Nimbus\Tests\Jobs;

use Nimbus\Jobs\SerializableJob;

final readonly class ReportJob implements SerializableJob
{
    public function __construct(public int $reportId)
    {
    }

    public static function jobName(): string
    {
        return 'reports.send';
    }

    public function toJobPayload(): array
    {
        return ['reportId' => $this->reportId];
    }

    public static function fromJobPayload(array $payload): self
    {
        $reportId = $payload['reportId'] ?? null;
        if (!is_int($reportId)) {
            throw new \UnexpectedValueException('Report job payload is invalid.');
        }
        return new self($reportId);
    }
}
