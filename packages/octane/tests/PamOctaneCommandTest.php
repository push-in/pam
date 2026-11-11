<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use PHPUnit\Framework\TestCase;

final class PamOctaneCommandTest extends TestCase
{
    private string $project;

    private string $log;

    protected function setUp(): void
    {
        $this->project = sys_get_temp_dir().'/pam-octane-command-'.bin2hex(random_bytes(6)).'/My App';
        mkdir($this->project, 0o700, true);
        touch($this->project.'/artisan');
        $this->log = $this->project.'/calls.log';
    }

    protected function tearDown(): void
    {
        @unlink($this->log);
        @unlink($this->project.'/artisan');
        @unlink($this->project.'/fake-pam');
        @rmdir($this->project);
        @rmdir(dirname($this->project));
    }

    public function testStartAcceptsOctaneStyleOptionsAndUsesAProjectScopedName(): void
    {
        $calls = $this->invoke([
            'start', '--workers=4', '--max-requests=500', '--host=0.0.0.0', '--port', '9000',
            '--log-level=debug', '--attach',
        ]);

        self::assertCount(2, $calls);
        self::assertSame('delete', $calls[0][0]);
        $name = $calls[0][1];
        self::assertMatchesRegularExpression('/^octane-my-app-[0-9a-f]{8}$/', $name);
        self::assertSame(
            ['up', 'artisan', '--name', $name, '--workers', '4', '--max-requests', '500', '--attach',
                '--', 'pam:octane', '--host=0.0.0.0', '--port=9000'],
            $calls[1],
        );
    }

    public function testExplicitNameAndControlCommandsTargetTheSameApplication(): void
    {
        $start = $this->invoke(['start', '--name=billing']);
        self::assertSame(['up', 'artisan', '--name', 'billing', '--', 'pam:octane'], $start[1]);

        $reload = $this->invoke(['reload', '--name', 'billing']);
        self::assertSame([['reload', 'billing']], $reload);
    }

    public function testWatchRunsTheForegroundDevelopmentWatcher(): void
    {
        $calls = $this->invoke(['start', '--watch', '--port=8080']);

        self::assertSame([['dev', 'artisan', 'pam:octane', '--port=8080']], $calls);
    }

    public function testTaskWorkersAreForwardedToTheSupervisor(): void
    {
        $calls = $this->invoke(['start', '--task-workers=2']);
        self::assertSame(
            ['up', 'artisan', '--name', $calls[0][1], '--task-workers', '2', '--', 'pam:octane'],
            $calls[1],
        );

        $calls = $this->invoke(['start'], ['PAM_OCTANE_TASK_WORKERS' => 'auto']);
        self::assertSame(['--task-workers', 'auto'], array_slice($calls[1], 4, 2));

        $calls = $this->invoke(['start', '--task-workers', '3'], ['PAM_OCTANE_TASK_WORKERS' => '9']);
        self::assertSame(['--task-workers', '3', '--'], array_slice($calls[1], 4, 3));

        // Control commands never receive the start-only default.
        self::assertSame([['stop', $calls[0][1]]], $this->invoke(['stop'], ['PAM_OCTANE_TASK_WORKERS' => '2']));
    }

    /**
     * @param list<string> $arguments
     * @param array<string, string> $environment
     * @return list<list<string>>
     */
    private function invoke(array $arguments, array $environment = []): array
    {
        @unlink($this->log);
        $fake = $this->project.'/fake-pam';
        file_put_contents($fake, "#!/bin/sh\nprintf '%s\\037' \"\$@\" >> ".escapeshellarg($this->log)."\nprintf '\\n' >> ".escapeshellarg($this->log)."\n[ \"\$1\" = status ] && exit 1\nexit 0\n");
        chmod($fake, 0o700);

        $process = proc_open(
            array_merge([PHP_BINARY, dirname(__DIR__).'/bin/pam-octane'], $arguments),
            [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
            $this->project,
            // The inherited environment keeps PHPRC, PAM_HOME and the
            // extension directory that PAM's launcher sets when the suite
            // itself runs inside PAM.
            ['PAM_BINARY' => $fake] + $environment + getenv(),
        );
        self::assertIsResource($process);
        fclose($pipes[0]);
        $stderr = stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        self::assertSame(0, proc_close($process), (string) $stderr);

        $calls = [];
        foreach (file($this->log, FILE_IGNORE_NEW_LINES) ?: [] as $line) {
            $calls[] = array_values(array_filter(explode("\x1f", $line), static fn (string $part): bool => $part !== ''));
        }

        // Status probes for legacy fallbacks are an implementation detail.
        return array_values(array_filter($calls, static fn (array $call): bool => ($call[0] ?? '') !== 'status'));
    }
}
