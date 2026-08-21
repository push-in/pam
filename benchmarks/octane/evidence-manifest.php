<?php

declare(strict_types=1);

enum EvidenceSuite: int
{
    case Comparison = 1;
    case Matrix = 2;
    case Soak = 3;
    case Overload = 4;
    case ManagerRecovery = 5;
    case ManagerRecoveryComparison = 6;
    case ManagerRecoveryWorkerMatrix = 7;
    case ManagerRecoveryExtensionProfile = 8;
}

/** @return array<string, mixed> */
function decodeEvidenceJson(string $path): array
{
    $decoded = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    if (!is_array($decoded)) {
        throw new RuntimeException("evidence JSON must contain an object: {$path}");
    }
    $object = [];
    foreach ($decoded as $key => $value) {
        if (!is_string($key)) {
            throw new RuntimeException("evidence JSON object contains a non-string key: {$path}");
        }
        $object[$key] = $value;
    }

    return $object;
}

$directory = isset($argv[1]) ? rtrim($argv[1], '/') : '';
$suiteValue = filter_var($argv[2] ?? null, FILTER_VALIDATE_INT);
$verify = ($argv[3] ?? '') === '--verify';

if ($directory === '' || !is_dir($directory) || $suiteValue === false) {
    fwrite(STDERR, "usage: php evidence-manifest.php <results-directory> <suite-id: 1|2|3|4|5|6|7|8> [--verify]\n");
    exit(64);
}

try {
    $suite = EvidenceSuite::from($suiteValue);
} catch (ValueError) {
    fwrite(STDERR, "evidence suite id must be 1 (comparison), 2 (matrix), 3 (soak), 4 (overload), 5 (manager recovery), 6 (manager recovery comparison), 7 (manager recovery worker matrix), or 8 (manager recovery extension profile)\n");
    exit(64);
}

$manifestPath = $directory.'/evidence-manifest.json';
$artifactFiles = static function (string $root): array {
    $allowedExtensions = ['csv', 'json', 'log', 'txt'];
    $files = [];
    $iterator = new RecursiveIteratorIterator(
        new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS),
    );
    foreach ($iterator as $file) {
        if (!$file instanceof SplFileInfo || !$file->isFile()) {
            continue;
        }
        $path = $file->getPathname();
        if (basename($path) === 'evidence-manifest.json') {
            continue;
        }
        if (!in_array(strtolower($file->getExtension()), $allowedExtensions, true)) {
            continue;
        }
        $relative = str_replace('\\', '/', substr($path, strlen($root) + 1));
        $files[$relative] = $path;
    }
    ksort($files, SORT_STRING);

    return $files;
};
$describeArtifacts = static function (array $files): array {
    $artifacts = [];
    foreach ($files as $relative => $path) {
        $hash = hash_file('sha256', $path);
        if (!is_string($hash)) {
            throw new RuntimeException("cannot hash evidence artifact {$relative}");
        }
        $bytes = filesize($path);
        if (!is_int($bytes)) {
            throw new RuntimeException("cannot measure evidence artifact {$relative}");
        }
        $artifacts[] = [
            'path' => $relative,
            'bytes' => $bytes,
            'sha256' => $hash,
        ];
    }

    return $artifacts;
};

if ($verify) {
    if (!is_file($manifestPath)) {
        fwrite(STDERR, "evidence manifest is missing: {$manifestPath}\n");
        exit(1);
    }
    $manifest = decodeEvidenceJson($manifestPath);
    if (($manifest['schema_version'] ?? null) !== 1
        || ($manifest['suite_id'] ?? null) !== $suite->value
        || !is_array($manifest['artifacts'] ?? null)) {
        fwrite(STDERR, "evidence manifest schema or suite does not match\n");
        exit(1);
    }
    $actual = $describeArtifacts($artifactFiles($directory));
    if ($actual !== $manifest['artifacts']) {
        fwrite(STDERR, "evidence artifacts do not match their manifest\n");
        exit(1);
    }
    fwrite(STDOUT, "Verified ".count($actual)." evidence artifacts.\n");
    exit(0);
}

$metadataPath = $directory.'/metadata.json';
$metadata = is_file($metadataPath) ? decodeEvidenceJson($metadataPath) : [];
$reportPath = match ($suite) {
    EvidenceSuite::Comparison => $directory.'/report.json',
    EvidenceSuite::Matrix => $directory.'/matrix-report.json',
    EvidenceSuite::Soak => $directory.'/soak-report.json',
    EvidenceSuite::Overload => $directory.'/overload-report.json',
    EvidenceSuite::ManagerRecovery => $directory.'/recovery-report.json',
    EvidenceSuite::ManagerRecoveryComparison => $directory.'/comparison-report.json',
    EvidenceSuite::ManagerRecoveryWorkerMatrix => $directory.'/worker-matrix-report.json',
    EvidenceSuite::ManagerRecoveryExtensionProfile => $directory.'/extension-profile-report.json',
};
$report = is_file($reportPath) ? decodeEvidenceJson($reportPath) : [];
$measurementGate = is_array($report['measurement_gate'] ?? null)
    ? $report['measurement_gate']
    : [];
$dynamicGate = is_array($report['dynamic_gate'] ?? null) ? $report['dynamic_gate'] : [];
$gateCodes = is_array($report['gate_codes'] ?? null) ? $report['gate_codes'] : [];
$manifest = [
    'schema_version' => 1,
    'suite_id' => $suite->value,
    'generated_at' => gmdate(DATE_ATOM),
    'source' => $metadata['source'] ?? null,
    'parameters' => $metadata['parameters'] ?? null,
    'gates' => match ($suite) {
        EvidenceSuite::Comparison => [
            'measurement' => $measurementGate['passed'] ?? false,
            'dynamic' => ($dynamicGate['passed_frankenphp'] ?? false)
                && ($dynamicGate['p99_passed'] ?? false)
                && ($dynamicGate['zero_errors'] ?? false),
        ],
        EvidenceSuite::Matrix => $report['gates'] ?? null,
        EvidenceSuite::Soak => ['soak' => $report['passed'] ?? false],
        EvidenceSuite::Overload => ['overload' => $report['passed'] ?? false],
        EvidenceSuite::ManagerRecovery => [
            'success' => ($gateCodes['success'] ?? null) === 1,
            'latency' => ($gateCodes['latency'] ?? null) === 1,
            'detection' => ($gateCodes['detection'] ?? null) === 1,
            'backoff' => ($gateCodes['backoff'] ?? null) === 1,
            'readiness' => ($gateCodes['readiness'] ?? null) === 1,
            'resources' => ($gateCodes['resources'] ?? null) === 1,
        ],
        EvidenceSuite::ManagerRecoveryComparison => [
            'pam_success' => ($gateCodes['pam_success'] ?? null) === 1,
            'pm2_success' => ($gateCodes['pm2_success'] ?? null) === 1,
            'equivalent_rounds' => ($gateCodes['equivalent_rounds'] ?? null) === 1,
        ],
        EvidenceSuite::ManagerRecoveryWorkerMatrix => [
            'all_configurations' => ($gateCodes['all_configurations'] ?? null) === 1,
            'equivalent_rounds' => ($gateCodes['equivalent_rounds'] ?? null) === 1,
        ],
        EvidenceSuite::ManagerRecoveryExtensionProfile => [
            'composer_profile' => ($gateCodes['composer_profile'] ?? null) === 1,
            'both_profiles' => ($gateCodes['both_profiles'] ?? null) === 1,
            'equivalent_rounds' => ($gateCodes['equivalent_rounds'] ?? null) === 1,
            'isolated_engine_not_slower' => ($gateCodes['isolated_engine_not_slower'] ?? null) === 1,
        ],
    },
    'artifacts' => $describeArtifacts($artifactFiles($directory)),
];
$encoded = json_encode(
    $manifest,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR,
)."\n";
if (file_put_contents($manifestPath, $encoded) === false) {
    throw new RuntimeException("cannot write {$manifestPath}");
}
fwrite(STDOUT, $encoded);
