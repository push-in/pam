<?php

declare(strict_types=1);

$directory = $argv[1] ?? __DIR__.'/results';
$root = dirname(__DIR__, 2);
$command = static function (string $command): string {
    $value = shell_exec($command.' 2>/dev/null');

    return is_string($value) ? trim($value) : '';
};
$read = static function (string $path): ?string {
    $value = @file_get_contents($path);

    return is_string($value) ? trim($value) : null;
};
$pamBinary = getenv('PAM_BENCH_BINARY') ?: $root.'/target/release/pam';
$frankenImage = getenv('PAM_BENCH_FRANKEN_IMAGE') ?: '';
$swooleImage = getenv('PAM_BENCH_SWOOLE_IMAGE') ?: '';
$environment = static function (string $name, string $default): string {
    $value = getenv($name);

    return $value === false || $value === '' ? $default : $value;
};

$metadata = [
    'schema_version' => 1,
    'generated_at' => gmdate(DATE_ATOM),
    'source' => [
        'commit' => $command('git -C '.escapeshellarg($root).' rev-parse HEAD'),
        'dirty' => $command('git -C '.escapeshellarg($root).' status --porcelain') !== '',
    ],
    'host' => [
        'kernel' => $command('uname -srmo'),
        'cpu' => $command("lscpu | sed -n 's/^Model name:[[:space:]]*//p' | head -1"),
        'logical_cpus' => (int) $command('getconf _NPROCESSORS_ONLN'),
        'governor' => $read('/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor'),
        'turbo_disabled' => $read('/sys/devices/system/cpu/intel_pstate/no_turbo'),
        'load_average' => $read('/proc/loadavg'),
        'memory_bytes' => (int) $command("awk '/MemTotal/ {print \$2 * 1024}' /proc/meminfo"),
    ],
    'tools' => [
        'wrk' => $command('wrk --version | head -1'),
        'docker_server' => $command("docker version --format '{{.Server.Version}}'"),
        'pam_sha256' => is_file($pamBinary) ? hash_file('sha256', $pamBinary) : null,
        'pam_version' => $command(escapeshellarg($pamBinary).' --version'),
        'frankenphp_image' => $frankenImage,
        'frankenphp_repo_digest' => $frankenImage === '' ? '' : $command(
            'docker image inspect --format '.escapeshellarg('{{join .RepoDigests ","}}').' '.escapeshellarg($frankenImage),
        ),
        'openswoole_image' => $swooleImage,
        'openswoole_image_id' => $swooleImage === '' ? '' : $command(
            'docker image inspect --format '.escapeshellarg('{{.Id}}').' '.escapeshellarg($swooleImage),
        ),
        'openswoole_dockerfile_sha256' => hash_file('sha256', __DIR__.'/Dockerfile.openswoole'),
    ],
    'parameters' => [
        'workers' => (int) $environment('PAM_BENCH_WORKERS', '1'),
        'threads' => (int) $environment('PAM_BENCH_THREADS', '4'),
        'connections' => (int) $environment('PAM_BENCH_CONNECTIONS', '128'),
        'duration' => $environment('PAM_BENCH_DURATION', '15s'),
        'warmup_duration' => $environment('PAM_BENCH_WARMUP_DURATION', '5s'),
        'rounds' => (int) $environment('PAM_BENCH_ROUNDS', '3'),
        'cooldown_seconds' => (float) $environment('PAM_BENCH_COOLDOWN', '2'),
        'scenario_order' => 'round-rotated with per-scenario warmup',
        'server_cpuset' => getenv('PAM_BENCH_SERVER_CPUSET') ?: null,
        'load_cpuset' => getenv('PAM_BENCH_LOAD_CPUSET') ?: null,
        'runtime_order' => preg_split('/\s+/', trim(getenv('PAM_BENCH_RUNTIME_ORDER') ?: '')) ?: [],
    ],
];

file_put_contents(
    rtrim($directory, '/').'/metadata.json',
    json_encode($metadata, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n",
);
