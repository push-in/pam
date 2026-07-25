<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Enums\JobEventType;
use Pam\Laravel\Support\ConfigValue;

final class ObservabilityRegistry
{
    private int $requests = 0;
    private int $errors = 0;
    private int $queries = 0;
    private int $queryNanoseconds = 0;
    /** @var array<string, array{count: int, errors: int, totalNs: int, maxNs: int}> */
    private array $routes = [];
    /** @var list<array{sql: string, milliseconds: float}> */
    private array $slowQueries = [];
    /** @var list<array{kind: string, detail: string, observedAt: string}> */
    private array $stateViolations = [];
    /** @var list<array{type: int, job: string, observedAt: string}> */
    private array $jobs = [];
    /** @var array<string, int> */
    private array $requestQueryShapes = [];

    public function beginRequest(): void
    {
        $this->requestQueryShapes = [];
    }

    public function request(string $route, int $status, int $durationNanoseconds): void
    {
        ++$this->requests;
        if ($status >= 500) {
            ++$this->errors;
        }
        $metric = $this->routes[$route] ?? ['count' => 0, 'errors' => 0, 'totalNs' => 0, 'maxNs' => 0];
        ++$metric['count'];
        if ($status >= 500) {
            ++$metric['errors'];
        }
        $metric['totalNs'] += $durationNanoseconds;
        $metric['maxNs'] = max($metric['maxNs'], $durationNanoseconds);
        $this->routes[$route] = $metric;
        $this->trimRoutes();
    }

    public function query(string $sql, float $milliseconds): void
    {
        ++$this->queries;
        $this->queryNanoseconds += (int) round($milliseconds * 1_000_000);
        if ($milliseconds >= ConfigValue::float('pam.observability.slow_query_ms', 100)) {
            $this->slowQueries[] = ['sql' => $sql, 'milliseconds' => $milliseconds];
            $limit = max(1, ConfigValue::int('pam.observability.query_limit', 128));
            if (count($this->slowQueries) > $limit) {
                array_shift($this->slowQueries);
            }
        }
        $shape = preg_replace(["/'[^']*'/", '/\\b\\d+\\b/'], ['?', '?'], $sql) ?: $sql;
        $this->requestQueryShapes[$shape] = ($this->requestQueryShapes[$shape] ?? 0) + 1;
        $threshold = max(2, ConfigValue::int('pam.observability.n_plus_one_threshold', 8));
        if ($this->requestQueryShapes[$shape] === $threshold) {
            $this->stateViolation('n-plus-one', "Query shape repeated {$threshold} times: {$shape}");
        }
    }

    public function job(JobEventType $type, string $job): void
    {
        $this->jobs[] = ['type' => $type->value, 'job' => $job, 'observedAt' => gmdate(DATE_ATOM)];
        if (count($this->jobs) > 128) {
            array_shift($this->jobs);
        }
    }

    public function stateViolation(string $kind, string $detail): void
    {
        $this->stateViolations[] = [
            'kind' => $kind,
            'detail' => $detail,
            'observedAt' => gmdate(DATE_ATOM),
        ];
        if (count($this->stateViolations) > 128) {
            array_shift($this->stateViolations);
        }
    }

    /** @return array<string, mixed> */
    public function snapshot(): array
    {
        return [
            'requests' => $this->requests,
            'errors' => $this->errors,
            'queries' => $this->queries,
            'queryMilliseconds' => $this->queryNanoseconds / 1_000_000,
            'memoryBytes' => memory_get_usage(true),
            'peakMemoryBytes' => memory_get_peak_usage(true),
            'routes' => $this->routes,
            'slowQueries' => $this->slowQueries,
            'stateViolations' => $this->stateViolations,
            'jobs' => $this->jobs,
        ];
    }

    public function memoryBytes(): int
    {
        return memory_get_usage(true);
    }

    public function peakMemoryBytes(): int
    {
        return memory_get_peak_usage(true);
    }

    public function stateViolationCount(): int
    {
        return count($this->stateViolations);
    }

    public function slowQueryCount(): int
    {
        return count($this->slowQueries);
    }

    public function hasStateViolations(): bool
    {
        return $this->stateViolations !== [];
    }

    private function trimRoutes(): void
    {
        $limit = max(1, ConfigValue::int('pam.observability.route_limit', 256));
        if (count($this->routes) <= $limit) {
            return;
        }
        uasort($this->routes, static fn (array $left, array $right): int => $left['count'] <=> $right['count']);
        array_shift($this->routes);
    }
}
