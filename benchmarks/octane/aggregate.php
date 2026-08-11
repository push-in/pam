<?php

declare(strict_types=1);

$directory = $argv[1] ?? __DIR__.'/results';
$files = glob(rtrim($directory, '/').'/*.round-*.json') ?: [];
$runtimes = [];

foreach ($files as $file) {
    $result = json_decode((string) file_get_contents($file), true, flags: JSON_THROW_ON_ERROR);
    if (!is_array($result) || !is_string($result['runtime'] ?? null)) {
        continue;
    }
    $runtimes[$result['runtime']][] = $result;
}

$median = static function (array $values): float {
    sort($values, SORT_NUMERIC);
    $count = count($values);
    if ($count === 0) {
        throw new RuntimeException('Cannot calculate a median without values.');
    }
    $middle = intdiv($count, 2);

    return $count % 2 === 1
        ? (float) $values[$middle]
        : ((float) $values[$middle - 1] + (float) $values[$middle]) / 2;
};

$summary = [];
foreach ($runtimes as $runtime => $rounds) {
    $rpsValues = array_map('floatval', array_column($rounds, 'rps'));
    $rpsMedian = $median($rpsValues);
    $absoluteDeviations = array_map(
        static fn (float $value): float => abs($value - $rpsMedian),
        $rpsValues,
    );
    $summary[$runtime] = [
        'rounds' => count($rounds),
        'median_rps' => $rpsMedian,
        'rps_relative_mad' => $rpsMedian > 0 ? $median($absoluteDeviations) / $rpsMedian : 1.0,
        'median_p99_us' => $median(array_map(
            static fn (array $round): int => (int) $round['latency']['p99_us'],
            $rounds,
        )),
        'errors' => array_sum(array_column($rounds, 'errors')),
    ];
}

$comparison = [];
$pairs = [
    'uncached_frankenphp' => ['pam-uncached', 'frankenphp-uncached'],
    'uncached_openswoole' => ['pam-uncached', 'openswoole-uncached'],
    'blade_frankenphp' => ['pam-blade', 'frankenphp-blade'],
    'blade_openswoole' => ['pam-blade', 'openswoole-blade'],
    'database_frankenphp' => ['pam-database', 'frankenphp-database'],
    'database_openswoole' => ['pam-database', 'openswoole-database'],
    'large_json_frankenphp' => ['pam-large-json', 'frankenphp-large-json'],
    'large_json_openswoole' => ['pam-large-json', 'openswoole-large-json'],
    'edge_cache_frankenphp' => ['pam-edge-cache', 'frankenphp-edge-comparison'],
    'edge_cache_openswoole' => ['pam-edge-cache', 'openswoole-edge-comparison'],
];
foreach ($pairs as $name => [$pamRuntime, $competitorRuntime]) {
    $pamRps = $summary[$pamRuntime]['median_rps'] ?? null;
    $competitorRps = $summary[$competitorRuntime]['median_rps'] ?? null;
    if (is_float($pamRps) && is_float($competitorRps) && $competitorRps > 0) {
        $comparison["{$name}_rps_ratio"] = round($pamRps / $competitorRps, 3);
    }
    $pamP99 = $summary[$pamRuntime]['median_p99_us'] ?? null;
    $competitorP99 = $summary[$competitorRuntime]['median_p99_us'] ?? null;
    if (is_float($pamP99) && is_float($competitorP99) && $competitorP99 > 0) {
        $comparison["{$name}_p99_ratio"] = round($pamP99 / $competitorP99, 3);
    }
}

$dynamicFrankenRatios = array_filter(
    $comparison,
    static fn (string $name): bool => str_ends_with($name, '_frankenphp_rps_ratio')
        && !str_starts_with($name, 'edge_cache_'),
    ARRAY_FILTER_USE_KEY,
);
$minimumDynamicRatio = (float) (getenv('PAM_BENCH_MIN_DYNAMIC_RATIO') ?: 0.80);
$failedDynamicScenarios = array_keys(array_filter(
    $dynamicFrankenRatios,
    static fn (float $ratio): bool => $ratio < $minimumDynamicRatio,
));
$dynamicFrankenP99Ratios = array_filter(
    $comparison,
    static fn (string $name): bool => str_ends_with($name, '_frankenphp_p99_ratio')
        && !str_starts_with($name, 'edge_cache_'),
    ARRAY_FILTER_USE_KEY,
);
$maximumDynamicP99Micros = max(
    1,
    (int) (getenv('PAM_BENCH_MAX_DYNAMIC_P99_US') ?: 100_000),
);
$dynamicPamSummaries = array_filter(
    $summary,
    static fn (string $name): bool => str_starts_with($name, 'pam-')
        && $name !== 'pam-edge-cache',
    ARRAY_FILTER_USE_KEY,
);
$failedP99Scenarios = array_keys(array_filter(
    $dynamicPamSummaries,
    static fn (array $runtime): bool => $runtime['median_p99_us'] > $maximumDynamicP99Micros,
));
$requiredRounds = max(5, (int) (getenv('PAM_BENCH_MIN_ROUNDS') ?: 5));
$unstableRuntimes = array_keys(array_filter(
    $summary,
    static fn (array $runtime): bool => $runtime['rounds'] < $requiredRounds
        || $runtime['rps_relative_mad'] > 0.05,
));
$metadataPath = rtrim($directory, '/').'/metadata.json';
$metadata = is_file($metadataPath)
    ? json_decode((string) file_get_contents($metadataPath), true, flags: JSON_THROW_ON_ERROR)
    : [];
$runtimeIdentities = [];
foreach (['pam', 'frankenphp', 'openswoole'] as $runtime) {
    $path = rtrim($directory, '/')."/runtime.{$runtime}.json";
    if (is_file($path) && filesize($path) > 0) {
        $runtimeIdentities[$runtime] = json_decode(
            (string) file_get_contents($path),
            true,
            flags: JSON_THROW_ON_ERROR,
        );
    }
}
$phpVersions = array_unique(array_column($runtimeIdentities, 'php_version'));
$runtimePreflightPassed = count($runtimeIdentities) === 3
    && count($phpVersions) === 1
    && array_all($runtimeIdentities, static fn (array $identity): bool =>
        ($identity['opcache'] ?? false) === true
        && ($identity['jit_enabled'] ?? false) === true
        && ($identity['debug'] ?? true) === false
    );
$cleanSource = ($metadata['source']['dirty'] ?? true) === false;

$report = [
    'schema_version' => 1,
    'generated_at' => gmdate(DATE_ATOM),
    'summary' => $summary,
    'comparison' => $comparison,
    'five_x_gate' => [
        'target' => 5.0,
        'scenario' => 'opt-in PAM edge cache versus full Laravel execution',
        'passed_frankenphp' => ($comparison['edge_cache_frankenphp_rps_ratio'] ?? 0) >= 5.0,
    ],
    'dynamic_gate' => [
        'minimum_throughput_ratio' => $minimumDynamicRatio,
        'scenario' => 'JSON, Blade, SQLite and large JSON with equal workers and equivalent OPcache/JIT settings',
        'passed_frankenphp' => $dynamicFrankenRatios !== [] && $failedDynamicScenarios === [],
        'minimum_frankenphp_ratio' => $dynamicFrankenRatios === [] ? 0 : min($dynamicFrankenRatios),
        'failed_scenarios' => $failedDynamicScenarios,
        'p99_maximum_microseconds' => $maximumDynamicP99Micros,
        'p99_passed' => $dynamicPamSummaries !== [] && $failedP99Scenarios === [],
        'failed_p99_scenarios' => $failedP99Scenarios,
        'zero_errors' => array_sum(array_column($summary, 'errors')) === 0,
    ],
    'measurement_gate' => [
        'required_rounds' => $requiredRounds,
        'maximum_relative_mad' => 0.05,
        'passed' => $unstableRuntimes === [] && $runtimePreflightPassed && $cleanSource,
        'unstable_or_incomplete_runtimes' => $unstableRuntimes,
        'runtime_preflight_passed' => $runtimePreflightPassed,
        'same_php_version' => count($phpVersions) === 1 ? reset($phpVersions) : false,
        'clean_source_required' => true,
        'clean_source' => $cleanSource,
    ],
    'runtime_identities' => $runtimeIdentities,
];

$encoded = json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n";
file_put_contents(rtrim($directory, '/').'/report.json', $encoded);
fwrite(STDOUT, $encoded);
