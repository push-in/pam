<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Database\Events\QueryExecuted;

final class QueryMonitor
{
    /** @var \WeakMap<object, QueryScope> */
    private \WeakMap $fiberScopes;

    private ?QueryScope $mainScope = null;

    public function __construct(FiberConnectionResolver $connections)
    {
        $this->fiberScopes = new \WeakMap();
        $connections->eventDispatcher()->listen(
            QueryExecuted::class,
            function (QueryExecuted $event): void {
                $scope = $this->currentScope();
                if ($scope === null) {
                    return;
                }
                ++$scope->count;
                $scope->elapsedMilliseconds += $event->time;
                $fingerprint = self::fingerprint($event->sql);
                $scope->duplicates[$fingerprint] = ($scope->duplicates[$fingerprint] ?? 0) + 1;
            },
        );
    }

    public function begin(QueryBudget $budget): void
    {
        if ($this->currentScope() !== null) {
            throw new \LogicException('A query budget is already active for this request Fiber.');
        }
        $scope = new QueryScope($budget);
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $this->mainScope = $scope;
            return;
        }
        $this->fiberScopes[$fiber] = $scope;
    }

    public function finish(): QueryReport
    {
        $scope = $this->currentScope()
            ?? throw new \LogicException('No query budget is active for this request Fiber.');
        $duplicates = array_filter(
            $scope->duplicates,
            static fn (int $count): bool => $count > 1,
        );
        arsort($duplicates, SORT_NUMERIC);
        $violations = [];
        if ($scope->count > $scope->budget->maximumQueries) {
            $violations[] = QueryBudgetViolation::QueryCount;
        }
        if ($scope->elapsedMilliseconds > $scope->budget->maximumElapsedMilliseconds) {
            $violations[] = QueryBudgetViolation::ElapsedTime;
        }
        if ($duplicates !== [] && max($duplicates) > $scope->budget->maximumDuplicateQueries) {
            $violations[] = QueryBudgetViolation::DuplicateQuery;
        }
        $this->clearCurrentScope();
        return new QueryReport(
            $scope->count,
            round($scope->elapsedMilliseconds, 3),
            $duplicates,
            $violations,
        );
    }

    private function currentScope(): ?QueryScope
    {
        $fiber = \Fiber::getCurrent();
        return $fiber === null ? $this->mainScope : ($this->fiberScopes[$fiber] ?? null);
    }

    private function clearCurrentScope(): void
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $this->mainScope = null;
            return;
        }
        unset($this->fiberScopes[$fiber]);
    }

    private static function fingerprint(string $sql): string
    {
        $normalized = preg_replace('/\s+/', ' ', trim($sql));
        return hash('xxh3', $normalized ?? $sql);
    }
}
