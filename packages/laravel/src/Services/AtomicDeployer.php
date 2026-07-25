<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use RuntimeException;

final readonly class AtomicDeployer
{
    public function prepare(string $release): void
    {
        $realRelease = $this->validateRelease($release);
        $binary = (string) config('pam.deploy.binary', 'pam');
        $this->run([$binary, 'artisan', 'optimize'], $realRelease);
        if ((bool) config('pam.deploy.migrate', true)) {
            $this->run([$binary, 'artisan', 'migrate', '--force', '--no-interaction'], $realRelease);
        }
    }

    public function activate(string $release): string
    {
        $realRelease = $this->validateRelease($release);
        $root = (string) realpath((string) config('pam.deploy.root'));

        $current = (string) config('pam.deploy.current');
        $previous = $this->previousTarget($current);
        $temporary = $current.'.next-'.getmypid();
        @unlink($temporary);
        if (!symlink($realRelease, $temporary) || !rename($temporary, $current)) {
            @unlink($temporary);
            throw new RuntimeException('Could not atomically activate the release.');
        }
        if ($previous !== null) {
            file_put_contents(rtrim($root, '/').'/.pam-previous', $previous, LOCK_EX);
        }

        return $realRelease;
    }

    public function ready(): bool
    {
        $url = (string) config('pam.deploy.readiness_url');
        $timeout = max(1, (int) config('pam.deploy.readiness_timeout_seconds', 30));
        $deadline = microtime(true) + $timeout;
        do {
            $context = stream_context_create(['http' => ['timeout' => 1, 'ignore_errors' => true]]);
            $body = @file_get_contents($url, false, $context);
            $status = $http_response_header[0] ?? '';
            if (is_string($body) && str_contains($status, ' 200 ')) {
                return true;
            }
            usleep(250_000);
        } while (microtime(true) < $deadline);

        return false;
    }

    public function rollback(): string
    {
        $root = realpath((string) config('pam.deploy.root'));
        $previous = $root === false ? '' : trim((string) @file_get_contents($root.'/.pam-previous'));
        if ($previous === '' || !is_dir($previous)) {
            throw new RuntimeException('No valid previous release is recorded.');
        }

        return $this->activate($previous);
    }

    private function previousTarget(string $current): ?string
    {
        $target = is_link($current) ? readlink($current) : false;

        return is_string($target) ? (realpath($target) ?: null) : null;
    }

    private function validateRelease(string $release): string
    {
        $realRelease = realpath($release);
        $root = realpath((string) config('pam.deploy.root'));
        if ($realRelease === false || !is_dir($realRelease) || $root === false) {
            throw new RuntimeException('Release and deploy root must exist.');
        }
        if (!str_starts_with($realRelease.'/', rtrim($root, '/').'/')) {
            throw new RuntimeException('Release must be inside PAM_DEPLOY_ROOT.');
        }
        if (!is_file($realRelease.'/artisan') || !is_file($realRelease.'/composer.lock')) {
            throw new RuntimeException('Release must contain artisan and composer.lock.');
        }

        return $realRelease;
    }

    /** @param list<string> $command */
    private function run(array $command, string $workingDirectory): void
    {
        $process = proc_open($command, [STDIN, STDOUT, STDERR], $pipes, $workingDirectory);
        if (!is_resource($process) || proc_close($process) !== 0) {
            throw new RuntimeException('Release preparation failed: '.implode(' ', $command));
        }
    }
}
