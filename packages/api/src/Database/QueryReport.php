<?php

declare(strict_types=1);

namespace Pam\Api\Database;

final readonly class QueryReport
{
    /**
     * @param array<string, int> $duplicates
     * @param list<QueryBudgetViolation> $violations
     */
    public function __construct(
        public int $count,
        public float $elapsedMilliseconds,
        public array $duplicates,
        public array $violations,
    ) {
    }

    public function withinBudget(): bool
    {
        return $this->violations === [];
    }
}
