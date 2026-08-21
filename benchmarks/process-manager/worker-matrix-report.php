<?php

declare(strict_types=1);

enum WorkerMatrixGate: int
{
    case Passed = 1;
    case Failed = 2;
}

$directory = isset($argv[1]) ? rtrim($argv[1], '/') : '';
if ($directory === '' || !is_dir($directory) || is_link($directory)) {
    fwrite(STDERR, "usage: worker-matrix-report.php RESULTS\n");
    exit(64);
}

$expectedWorkers = [1, 4, 16];
$configurations = [];
$source = null;
$host = null;
$tools = null;
$rounds = null;
$allPassed = true;
$equivalentRounds = true;
foreach ($expectedWorkers as $workers) {
    $configurationDirectory = $directory.'/workers-'.$workers;
    $reportPath = $configurationDirectory.'/recovery-report.json';
    $metadataPath = $configurationDirectory.'/metadata.json';
    if (!is_file($reportPath) || is_link($reportPath) || filesize($reportPath) > 256 * 1024
        || !is_file($metadataPath) || is_link($metadataPath) || filesize($metadataPath) > 256 * 1024) {
        fwrite(STDERR, "worker-matrix evidence is missing, unsafe, or oversized for {$workers} workers\n");
        exit(1);
    }
    $report = json_decode((string) file_get_contents($reportPath), true, flags: JSON_THROW_ON_ERROR);
    $metadata = json_decode((string) file_get_contents($metadataPath), true, flags: JSON_THROW_ON_ERROR);
    $configurationRounds = $report['rounds'] ?? null;
    if (($report['schema_version'] ?? null) !== 1 || ($report['suite_code'] ?? null) !== 5
        || !is_int($configurationRounds) || $configurationRounds < 3
        || ($metadata['parameters']['workers'] ?? null) !== $workers
        || ($metadata['parameters']['rounds'] ?? null) !== $configurationRounds
        || !is_array($metadata['source'] ?? null) || !is_array($metadata['host'] ?? null)
        || !is_array($metadata['tools'] ?? null)) {
        fwrite(STDERR, "worker-matrix contract is invalid for {$workers} workers\n");
        exit(1);
    }
    if ($source === null) {
        $source = $metadata['source'];
        $host = $metadata['host'];
        $tools = $metadata['tools'];
        $rounds = $configurationRounds;
    } elseif ($source !== $metadata['source'] || $host !== $metadata['host']
        || $tools !== $metadata['tools']) {
        fwrite(STDERR, "worker-matrix provenance differs between configurations\n");
        exit(1);
    }
    $equivalentRounds = $equivalentRounds && $configurationRounds === $rounds;
    $configurationPassed = ($report['passed'] ?? false) === true;
    $allPassed = $allPassed && $configurationPassed;
    $configurations[] = [
        'configuration_code' => count($configurations) + 1,
        'workers' => $workers,
        'rounds' => $configurationRounds,
        'successful_rounds' => $report['successful_rounds'] ?? null,
        'recovery_millis' => $report['recovery_millis'] ?? null,
        'recovery_phases' => $report['recovery_phases'] ?? null,
        'daemon_rss_growth_bytes' => $report['daemon_rss_growth_bytes'] ?? null,
        'thresholds' => $report['thresholds'] ?? null,
        'gate_codes' => $report['gate_codes'] ?? null,
        'passed' => $configurationPassed,
    ];
}

$unexpected = glob($directory.'/workers-*', GLOB_ONLYDIR) ?: [];
if (count($unexpected) !== count($expectedWorkers)) {
    fwrite(STDERR, "worker-matrix contains an unexpected configuration\n");
    exit(1);
}
$passed = $allPassed && $equivalentRounds;
$report = [
    'schema_version' => 1,
    'suite_code' => 7,
    'configurations' => $configurations,
    'gate_codes' => [
        'all_configurations' => ($allPassed ? WorkerMatrixGate::Passed : WorkerMatrixGate::Failed)->value,
        'equivalent_rounds' => ($equivalentRounds ? WorkerMatrixGate::Passed : WorkerMatrixGate::Failed)->value,
    ],
    'passed' => $passed,
];
$metadata = [
    'schema_version' => 1,
    'source' => $source,
    'host' => $host,
    'tools' => $tools,
    'parameters' => [
        'rounds_per_configuration' => $rounds,
        'worker_configurations' => $expectedWorkers,
    ],
];
foreach (['worker-matrix-report.json' => $report, 'metadata.json' => $metadata] as $name => $value) {
    $path = $directory.'/'.$name;
    if (is_file($path) || is_link($path)) {
        fwrite(STDERR, "refusing to overwrite worker-matrix artifact: {$path}\n");
        exit(1);
    }
    file_put_contents($path, json_encode(
        $value,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR,
    )."\n");
}
fwrite(STDOUT, json_encode($report, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n");
exit($passed ? 0 : 1);
