<?php

declare(strict_types=1);

namespace Pam\Api\Database;

final readonly class QueryBudget
{
    public function __construct(
        public int $maximumQueries = 100,
        public float $maximumElapsedMilliseconds = 500.0,
        public int $maximumDuplicateQueries = 10,
    ) {
        if ($maximumQueries < 1 || $maximumElapsedMilliseconds <= 0 || $maximumDuplicateQueries < 1) {
            throw new \InvalidArgumentException('Query budgets must use positive limits.');
        }
    }
}
