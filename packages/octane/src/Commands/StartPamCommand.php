<?php

declare(strict_types=1);

namespace Pam\Octane\Commands;

use Illuminate\Support\Str;
use Laravel\Octane\Commands\Command;
use Laravel\Octane\Commands\Concerns\InteractsWithEnvironmentVariables;
use Laravel\Octane\Commands\Concerns\InteractsWithServers;
use Pam\Octane\Commands\Concerns\InstallsPamDependencies;
use Pam\Octane\Server\ServerProcessInspector;
use Pam\Octane\Server\ServerStateFile;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\SignalableCommandInterface;
use Symfony\Component\Process\Process;

/**
 * Runs `php artisan octane:start --server=pam`: a foreground `pam start`
 * cluster supervised like Octane's other servers (reload, stop, status,
 * --watch). Each worker boots the application once through
 * bin/pam-octane-worker.php.
 */
#[AsCommand(name: 'octane:pam')]
class StartPamCommand extends Command implements SignalableCommandInterface
{
    use InstallsPamDependencies;
    use InteractsWithEnvironmentVariables;
    use InteractsWithServers;

    /** @var string */
    public $signature = 'octane:pam
                    {--host= : The IP address the server should bind to}
                    {--port= : The port the server should be available on}
                    {--workers=auto : The number of workers that should be available to handle requests}
                    {--task-workers=0 : The number of task workers that should be available to handle tasks and ticks}
                    {--max-requests=500 : The number of requests to process before reloading the server}
                    {--watch : Automatically reload the server when the application is modified}
                    {--poll : Use file system polling while watching in order to watch files over a network}';

    /** @var string */
    public $description = 'Start the Octane PAM server';

    /** @var bool */
    protected $hidden = true;

    public function handle(ServerProcessInspector $inspector, ServerStateFile $serverStateFile): int
    {
        $pamBinary = $this->ensurePamBinaryIsInstalled();

        if ($inspector->serverIsRunning()) {
            $this->components->error('PAM server is already running.');

            return 1;
        }

        $this->ensurePortIsAvailable();

        $this->ensurePamBinaryMeetsRequirements($pamBinary);

        $this->writeServerStateFile($serverStateFile);

        $this->forgetEnvironmentVariables();

        $server = new Process($this->serverCommand($pamBinary), base_path(), [
            'APP_ENV' => (string) app()->environment(),
            'APP_BASE_PATH' => base_path(),
            'LARAVEL_OCTANE' => '1',
            'OCTANE_HOST' => (string) $this->getHost(),
            'OCTANE_PORT' => (string) $this->getPort(),
            'PAM_OCTANE_STREAM_REQUESTS' => '1',
        ], null, null);
        $server->start();

        $pid = $server->getPid();
        if ($pid !== null) {
            $serverStateFile->writeProcessId($pid);
        }

        return $this->runServer($server, $inspector, 'pam');
    }

    /** @return list<string> */
    protected function serverCommand(string $pamBinary): array
    {
        $workerScript = realpath(__DIR__.'/../../bin/pam-octane-worker.php');

        return [
            $pamBinary,
            'start',
            $workerScript === false ? __DIR__.'/../../bin/pam-octane-worker.php' : $workerScript,
            ...($this->workerCount() > 0 ? ['--workers', (string) $this->workerCount()] : []),
            '--max-requests', $this->stringOption('max-requests', '500'),
            ...(($taskWorkers = $this->taskWorkers()) !== null ? ['--task-workers', $taskWorkers] : []),
        ];
    }

    protected function writeServerStateFile(ServerStateFile $serverStateFile): void
    {
        $serverStateFile->writeState([
            'appName' => config('app.name', 'Laravel'),
            'host' => $this->getHost(),
            'port' => $this->getPort(),
            'workers' => $this->workerCount(),
            'taskWorkers' => $this->taskWorkers(),
            'maxRequests' => $this->stringOption('max-requests', '500'),
            'octaneConfig' => config('octane'),
        ]);
    }

    /** The number of HTTP workers; 0 lets PAM start one per CPU. */
    protected function workerCount(): int
    {
        $workers = $this->stringOption('workers', 'auto');

        return $workers === 'auto' ? 0 : max(0, (int) $workers);
    }

    /** Task workers are opt-in on PAM ("auto" starts one per CPU). */
    protected function taskWorkers(): ?string
    {
        $taskWorkers = $this->stringOption('task-workers', '0');

        if ($taskWorkers === 'auto') {
            return 'auto';
        }

        return (int) $taskWorkers > 0 ? (string) (int) $taskWorkers : null;
    }

    /**
     * Write the server process output to the console.
     *
     * @param Process $server
     */
    protected function writeServerOutput($server): void
    {
        [$output, $errorOutput] = $this->getServerOutput($server);

        Str::of(is_string($output) ? $output : '')
            ->explode("\n")
            ->filter()
            ->each(function (string $line): void {
                $stream = json_decode($line, true);

                is_array($stream)
                    ? $this->handleStream($stream)
                    : $this->components->info($line);
            });

        Str::of(is_string($errorOutput) ? $errorOutput : '')
            ->explode("\n")
            ->filter()
            ->reject(fn (string $line): bool => Str::contains($line, ['READY', 'Pam listening on', 'zero-downtime reload'])
                || preg_match('/worker=\d+\s*$/', $line) === 1)
            ->each(function (string $line): void {
                $debug = json_decode($line, true);

                if (!is_array($debug)) {
                    $this->raw($line);

                    return;
                }

                if (isset($debug['type'])) {
                    $this->handleStream($debug);

                    return;
                }

                // Requests are already written by the Octane worker...
                if (($debug['event'] ?? null) === 'http.request') {
                    return;
                }

                $message = $debug['message'] ?? $debug['event'] ?? $line;
                $message = is_string($message) ? $message : $line;

                match ($debug['level'] ?? 'info') {
                    'error' => $this->components->error($message),
                    'warn', 'warning' => $this->components->warn($message),
                    default => $this->components->info($message),
                };
            });
    }

    protected function stopServer(): void
    {
        $this->callSilent('octane:stop', [
            '--server' => 'pam',
        ]);
    }

    private function stringOption(string $name, string $default): string
    {
        $value = $this->option($name);

        return is_scalar($value) && (string) $value !== '' ? (string) $value : $default;
    }
}
