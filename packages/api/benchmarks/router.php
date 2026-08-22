<?php

declare(strict_types=1);

require dirname(__DIR__) . '/vendor/autoload.php';

use Pam\Api\RouteConstraint;
use Pam\Api\Router;

const ROUTER_BENCHMARK_SCHEMA = 1;
const ROUTER_BENCHMARK_ITERATIONS = 50_000;
const ROUTER_BENCHMARK_WARMUP = 5_000;
const MINIMUM_MATCHES_PER_SECOND = 50_000;
const MAXIMUM_P99_NANOSECONDS = 500_000;
const MAXIMUM_PEAK_MEMORY_BYTES = 67_108_864;

$assertBudget = in_array('--assert', array_slice($argv, 1), true);
$router = new Router();
for ($index = 1; $index <= 100; ++$index) {
    $router->add('GET', "/static/{$index}", static fn (): null => null);
}
$dynamic = $router->register('GET', '/users/{id}', static fn (): null => null);
$router->constrain($dynamic, 'id', RouteConstraint::Integer);

for ($index = 0; $index < ROUTER_BENCHMARK_WARMUP; ++$index) {
    $router->match('GET', '/static/50');
    $router->match('GET', '/users/42');
}

$static = measureRoute($router, '/static/50');
$dynamicResult = measureRoute($router, '/users/42');
$peakMemory = memory_get_peak_usage(true);
$passed = $static['matchesPerSecond'] >= MINIMUM_MATCHES_PER_SECOND
    && $dynamicResult['matchesPerSecond'] >= MINIMUM_MATCHES_PER_SECOND
    && $static['p99Nanoseconds'] <= MAXIMUM_P99_NANOSECONDS
    && $dynamicResult['p99Nanoseconds'] <= MAXIMUM_P99_NANOSECONDS
    && $peakMemory <= MAXIMUM_PEAK_MEMORY_BYTES;

echo json_encode([
    'schemaVersion' => ROUTER_BENCHMARK_SCHEMA,
    'phpVersion' => PHP_VERSION,
    'routes' => 101,
    'warmupIterations' => ROUTER_BENCHMARK_WARMUP,
    'measuredIterationsPerPath' => ROUTER_BENCHMARK_ITERATIONS,
    'static' => $static,
    'dynamic' => $dynamicResult,
    'peakMemoryBytes' => $peakMemory,
    'budget' => [
        'minimumMatchesPerSecond' => MINIMUM_MATCHES_PER_SECOND,
        'maximumP99Nanoseconds' => MAXIMUM_P99_NANOSECONDS,
        'maximumPeakMemoryBytes' => MAXIMUM_PEAK_MEMORY_BYTES,
    ],
    'passed' => $passed,
], JSON_THROW_ON_ERROR | JSON_PRETTY_PRINT), "\n";

if ($assertBudget && !$passed) {
    fwrite(STDERR, "Router benchmark exceeded its regression budget.\n");
    exit(1);
}

/**
 * @return array{
 *     elapsedSeconds: float,
 *     matchesPerSecond: int,
 *     p50Nanoseconds: int,
 *     p95Nanoseconds: int,
 *     p99Nanoseconds: int
 * }
 */
function measureRoute(Router $router, string $path): array
{
    $samples = [];
    $startedAt = hrtime(true);
    for ($index = 0; $index < ROUTER_BENCHMARK_ITERATIONS; ++$index) {
        $sampleStartedAt = hrtime(true);
        $result = $router->match('GET', $path);
        if ($result->route === null) {
            throw new RuntimeException("Benchmark route {$path} did not match.");
        }
        $samples[] = hrtime(true) - $sampleStartedAt;
    }
    $elapsedSeconds = (hrtime(true) - $startedAt) / 1_000_000_000;
    sort($samples, SORT_NUMERIC);

    return [
        'elapsedSeconds' => $elapsedSeconds,
        'matchesPerSecond' => (int) round(ROUTER_BENCHMARK_ITERATIONS / $elapsedSeconds),
        'p50Nanoseconds' => percentile($samples, 50),
        'p95Nanoseconds' => percentile($samples, 95),
        'p99Nanoseconds' => percentile($samples, 99),
    ];
}

/** @param non-empty-list<int> $samples */
function percentile(array $samples, int $percentile): int
{
    $index = (int) ceil((count($samples) * $percentile) / 100) - 1;
    return $samples[max(0, min(count($samples) - 1, $index))];
}
