<?php

declare(strict_types=1);

namespace Pam\Api\Database;

final class QueryBudgetExceededException extends \RuntimeException
{
    public function __construct(public readonly QueryReport $report)
    {
        $codes = array_map(
            static fn (QueryBudgetViolation $violation): string => (string) $violation->value,
            $report->violations,
        );
        parent::__construct('Database query budget exceeded; violation codes: ' . implode(', ', $codes) . '.');
    }
}
