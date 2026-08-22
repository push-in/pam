<?php

declare(strict_types=1);

namespace Pam\Api\Database;

/** @internal Mutable state owned by one request Fiber. */
final class QueryScope
{
    public int $count = 0;

    public float $elapsedMilliseconds = 0.0;

    /** @var array<string, int> */
    public array $duplicates = [];

    public function __construct(public readonly QueryBudget $budget)
    {
    }
}
