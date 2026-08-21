<?php

declare(strict_types=1);

enum ExtensionProfileCode: int
{
    case Compatible = 1;
    case Isolated = 2;
}

enum ExtensionProfileGate: int
{
    case Passed = 1;
    case Failed = 2;
}

/** @return array<string, mixed> */
function decodeJsonObject(string $path): array
{
    $decoded = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    if (!is_array($decoded)) {
        throw new RuntimeException("JSON evidence must contain an object: {$path}");
    }
    $object = [];
    foreach ($decoded as $key => $value) {
        if (!is_string($key)) {
            throw new RuntimeException("JSON evidence object contains a non-string key: {$path}");
        }
        $object[$key] = $value;
    }

    return $object;
}

/**
 * @param array<string, mixed> $configuration
 * @param non-empty-list<string> $path
 */
function nestedInteger(array $configuration, array $path): int
{
    $value = $configuration;
    foreach ($path as $key) {
        if (!is_array($value) || !array_key_exists($key, $value)) {
            throw new RuntimeException("extension-profile metric is missing: {$key}");
        }
        $value = $value[$key];
    }
    if (!is_int($value)) {
        throw new RuntimeException('extension-profile metric must be an integer');
    }

    return $value;
}

$directory = isset($argv[1]) ? rtrim($argv[1], '/') : '';
if ($directory === '' || !is_dir($directory) || is_link($directory)) {
    fwrite(STDERR, "usage: extension-profile-report.php RESULTS\n");
    exit(64);
}

$profiles = [
    ExtensionProfileCode::Compatible->value => ['directory' => 'compatible', 'extensions' => []],
    ExtensionProfileCode::Isolated->value => ['directory' => 'isolated', 'extensions' => ['iconv']],
];
$configurations = [];
$source = null;
$host = null;
$tools = null;
$rounds = null;
$workers = null;
foreach ($profiles as $code => $expected) {
    $profileDirectory = $directory.'/'.$expected['directory'];
    $reportPath = $profileDirectory.'/recovery-report.json';
    $metadataPath = $profileDirectory.'/metadata.json';
    if (!is_file($reportPath) || is_link($reportPath) || filesize($reportPath) > 256 * 1024
        || !is_file($metadataPath) || is_link($metadataPath) || filesize($metadataPath) > 256 * 1024) {
        fwrite(STDERR, "extension-profile evidence is missing, unsafe, or oversized\n");
        exit(1);
    }
    $report = decodeJsonObject($reportPath);
    $metadata = decodeJsonObject($metadataPath);
    $parameters = $metadata['parameters'] ?? null;
    if (($report['schema_version'] ?? null) !== 1 || ($report['suite_code'] ?? null) !== 5
        || !is_array($parameters) || ($parameters['php_extensions'] ?? null) !== $expected['extensions']
        || !is_int($parameters['rounds'] ?? null) || !is_int($parameters['workers'] ?? null)
        || !is_array($report['worker_startup'] ?? null)
        || !is_array($metadata['source'] ?? null) || !is_array($metadata['host'] ?? null)
        || !is_array($metadata['tools'] ?? null)) {
        fwrite(STDERR, "extension-profile evidence contract is invalid\n");
        exit(1);
    }
    if ($source === null) {
        $source = $metadata['source'];
        $host = $metadata['host'];
        $tools = $metadata['tools'];
        $rounds = $parameters['rounds'];
        $workers = $parameters['workers'];
    } elseif ($source !== $metadata['source'] || $host !== $metadata['host']
        || $tools !== $metadata['tools'] || $rounds !== $parameters['rounds']
        || $workers !== $parameters['workers']) {
        fwrite(STDERR, "extension-profile provenance or parameters differ\n");
        exit(1);
    }
    $configurations[$code] = [
        'profile_code' => $code,
        'php_extensions' => $expected['extensions'],
        'rounds' => $report['rounds'] ?? null,
        'successful_rounds' => $report['successful_rounds'] ?? null,
        'recovery_millis' => $report['recovery_millis'] ?? null,
        'recovery_phases' => $report['recovery_phases'] ?? null,
        'worker_startup' => $report['worker_startup'],
        'daemon_rss_growth_bytes' => $report['daemon_rss_growth_bytes'] ?? null,
        'gate_codes' => $report['gate_codes'] ?? null,
        'passed' => ($report['passed'] ?? false) === true,
    ];
}

$compatible = $configurations[ExtensionProfileCode::Compatible->value];
$isolated = $configurations[ExtensionProfileCode::Isolated->value];
$compatibleTotal = nestedInteger($compatible, ['recovery_millis', 'p95']);
$isolatedTotal = nestedInteger($isolated, ['recovery_millis', 'p95']);
$compatibleReadiness = nestedInteger($compatible, ['recovery_phases', 'readiness_millis', 'p95']);
$isolatedReadiness = nestedInteger($isolated, ['recovery_phases', 'readiness_millis', 'p95']);
$compatibleEngine = nestedInteger($compatible, ['worker_startup', 'php_engine_p95_millis', 'p95']);
$isolatedEngine = nestedInteger($isolated, ['worker_startup', 'php_engine_p95_millis', 'p95']);
$improvement = static fn (int $before, int $after): int => $before === 0
    ? 0
    : (int) round((($before - $after) * 10_000) / $before);
$bothPassed = $compatible['passed'] && $isolated['passed'];
$equivalentRounds = $compatible['rounds'] === $isolated['rounds'];
$isolatedEngineNotSlower = $isolatedEngine <= $compatibleEngine;
$passed = $bothPassed && $equivalentRounds && $isolatedEngineNotSlower;
$report = [
    'schema_version' => 1,
    'suite_code' => 8,
    'workers' => $workers,
    'configurations' => array_values($configurations),
    'isolated_minus_compatible_millis' => [
        'recovery_p95' => $isolatedTotal - $compatibleTotal,
        'readiness_p95' => $isolatedReadiness - $compatibleReadiness,
        'php_engine_p95' => $isolatedEngine - $compatibleEngine,
    ],
    'isolated_improvement_basis_points' => [
        'recovery_p95' => $improvement($compatibleTotal, $isolatedTotal),
        'readiness_p95' => $improvement($compatibleReadiness, $isolatedReadiness),
        'php_engine_p95' => $improvement($compatibleEngine, $isolatedEngine),
    ],
    'gate_codes' => [
        'both_profiles' => ($bothPassed ? ExtensionProfileGate::Passed : ExtensionProfileGate::Failed)->value,
        'equivalent_rounds' => ($equivalentRounds ? ExtensionProfileGate::Passed : ExtensionProfileGate::Failed)->value,
        'isolated_engine_not_slower' => ($isolatedEngineNotSlower ? ExtensionProfileGate::Passed : ExtensionProfileGate::Failed)->value,
    ],
    'passed' => $passed,
];
$metadata = [
    'schema_version' => 1,
    'source' => $source,
    'host' => $host,
    'tools' => $tools,
    'parameters' => [
        'rounds_per_profile' => $rounds,
        'workers' => $workers,
        'profile_codes' => array_keys($profiles),
    ],
];
foreach (['extension-profile-report.json' => $report, 'metadata.json' => $metadata] as $name => $value) {
    $path = $directory.'/'.$name;
    if (is_file($path) || is_link($path)) {
        fwrite(STDERR, "refusing to overwrite extension-profile artifact: {$path}\n");
        exit(1);
    }
    file_put_contents($path, json_encode(
        $value,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR,
    )."\n");
}
fwrite(STDOUT, json_encode($report, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n");
exit($passed ? 0 : 1);
