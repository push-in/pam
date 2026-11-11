<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

use Illuminate\Contracts\Console\Kernel;
use Laravel\Octane\Commands as Octane;
use Laravel\Octane\Contracts\ServerProcessInspector as ServerProcessInspectorContract;
use Pam\Octane\Commands\Octane as Pam;
use Pam\Octane\Commands\StartPamCommand;
use Pam\Octane\Server\ServerProcessInspector;
use Pam\Octane\Server\ServerStateFile;
use PHPUnit\Framework\Attributes\DataProvider;

final class OctaneCommandsTest extends OctaneServerTestCase
{
    /** @return array<string, array{class-string, class-string, string}> */
    public static function commands(): array
    {
        return [
            'start' => [Octane\StartCommand::class, Pam\StartCommand::class, 'octane:start'],
            'reload' => [Octane\ReloadCommand::class, Pam\ReloadCommand::class, 'octane:reload'],
            'stop' => [Octane\StopCommand::class, Pam\StopCommand::class, 'octane:stop'],
            'status' => [Octane\StatusCommand::class, Pam\StatusCommand::class, 'octane:status'],
            'install' => [Octane\InstallCommand::class, Pam\InstallCommand::class, 'octane:install'],
        ];
    }

    /**
     * @param class-string $octane
     * @param class-string $pam
     */
    #[DataProvider('commands')]
    public function test_octane_commands_resolve_to_their_pam_aware_subclass(string $octane, string $pam, string $name): void
    {
        self::assertInstanceOf($pam, $this->application()->make($octane));
        self::assertInstanceOf($octane, $this->application()->make($octane));

        $kernel = $this->application()->make(Kernel::class);
        self::assertInstanceOf(Kernel::class, $kernel);
        self::assertInstanceOf($pam, $kernel->all()[$name] ?? null);
    }

    public function test_start_with_server_pam_starts_the_pam_server_with_octane_options(): void
    {
        $this->application()->bind(StartPamCommand::class, RecordingStartPamCommand::class);
        RecordingStartPamCommand::$options = [];

        $this->command('octane:start', [
            '--server' => 'pam',
            '--host' => '0.0.0.0',
            '--port' => '9001',
            '--workers' => '3',
            '--task-workers' => '2',
            '--max-requests' => '250',
            '--watch' => true,
        ])->assertExitCode(0);

        self::assertSame('0.0.0.0', RecordingStartPamCommand::recorded('host'));
        self::assertSame('9001', RecordingStartPamCommand::recorded('port'));
        self::assertSame('3', RecordingStartPamCommand::recorded('workers'));
        self::assertSame('2', RecordingStartPamCommand::recorded('task-workers'));
        self::assertSame('250', RecordingStartPamCommand::recorded('max-requests'));
        self::assertTrue(RecordingStartPamCommand::recorded('watch'));
    }

    public function test_start_uses_octane_configuration_defaults(): void
    {
        $this->application()->bind(StartPamCommand::class, RecordingStartPamCommand::class);
        RecordingStartPamCommand::$options = [];
        config([
            'octane.server' => 'pam',
            'octane.workers' => 4,
            'octane.max_requests' => 1000,
        ]);

        $this->command('octane:start')->assertExitCode(0);

        self::assertSame('127.0.0.1', RecordingStartPamCommand::recorded('host'));
        self::assertSame('8000', RecordingStartPamCommand::recorded('port'));
        self::assertEquals('4', RecordingStartPamCommand::recorded('workers'));
        self::assertEquals('0', RecordingStartPamCommand::recorded('task-workers'));
        self::assertEquals('1000', RecordingStartPamCommand::recorded('max-requests'));
        self::assertFalse(RecordingStartPamCommand::recorded('watch'));
    }

    public function test_the_pam_server_command_command_line(): void
    {
        $command = new InspectableStartPamCommand();
        $command->setLaravel($this->application());
        $command->useOptions(['--workers' => '4', '--task-workers' => 'auto', '--max-requests' => '750']);

        $line = $command->commandLine('/usr/local/bin/pam');

        self::assertSame('/usr/local/bin/pam', $line[0]);
        self::assertSame('start', $line[1]);
        self::assertStringEndsWith('/bin/pam-octane-worker.php', $line[2]);
        self::assertFileExists($line[2]);
        self::assertSame(['--workers', '4', '--max-requests', '750', '--task-workers', 'auto'], array_slice($line, 3));

        $command->useOptions([]);
        self::assertSame(['--max-requests', '500'], array_slice($command->commandLine('pam'), 3));
    }

    public function test_other_servers_are_left_to_octane(): void
    {
        $this->command('octane:status', ['--server' => 'unknown'])
            ->expectsOutputToContain('Invalid server: unknown.');
        $this->command('octane:reload', ['--server' => 'unknown'])
            ->expectsOutputToContain('Invalid server: unknown.');
        $this->command('octane:stop', ['--server' => 'unknown'])
            ->expectsOutputToContain('Invalid server: unknown.');
        $this->command('octane:start', ['--server' => 'unknown'])
            ->expectsOutputToContain('Invalid server: unknown.');
    }

    public function test_status_reports_a_stopped_server(): void
    {
        $this->command('octane:status', ['--server' => 'pam'])
            ->expectsOutputToContain('Octane server is not running.')
            ->assertExitCode(1);
    }

    public function test_octane_server_configuration_selects_pam(): void
    {
        config(['octane.server' => 'pam']);
        (new ServerStateFile($this->stateFile))->writeProcessId((int) getmypid());

        $this->command('octane:status')
            ->expectsOutputToContain('Octane server is running.')
            ->assertExitCode(0);
    }

    public function test_reload_and_stop_require_a_running_server(): void
    {
        $this->command('octane:reload', ['--server' => 'pam'])
            ->expectsOutputToContain('Octane server is not running.')
            ->assertExitCode(1);

        $this->command('octane:stop', ['--server' => 'pam'])
            ->expectsOutputToContain('PAM server is not running.')
            ->assertExitCode(1);
    }

    public function test_reload_signals_the_pam_supervisor(): void
    {
        $posix = $this->runningServer(4242);

        $this->command('octane:reload', ['--server' => 'pam'])
            ->expectsOutputToContain('Reloading workers')
            ->assertExitCode(0);

        self::assertContains([4242, SIGHUP], $posix->signals);
    }

    public function test_stop_signals_the_pam_supervisor_and_forgets_its_state(): void
    {
        $posix = $this->runningServer(4242);

        $this->command('octane:stop', ['--server' => 'pam'])
            ->expectsOutputToContain('Stopping server')
            ->assertExitCode(0);

        self::assertContains([4242, SIGTERM], $posix->signals);
        self::assertFileDoesNotExist($this->stateFile);
    }

    public function test_the_inspector_is_an_octane_server_process_inspector(): void
    {
        self::assertInstanceOf(ServerProcessInspectorContract::class, $this->application()->make(ServerProcessInspector::class));
        self::assertSame($this->stateFile, $this->application()->make(ServerStateFile::class)->path());
    }

    private function runningServer(int $processId): RecordingPosix
    {
        $stateFile = new ServerStateFile($this->stateFile);
        $stateFile->writeProcessId($processId);
        $posix = new RecordingPosix(true);
        $this->application()->instance(ServerProcessInspector::class, new ServerProcessInspector($stateFile, $posix));

        return $posix;
    }
}

final class RecordingStartPamCommand extends StartPamCommand
{
    /** @var array<string, mixed> */
    public static array $options = [];

    public function handle(ServerProcessInspector $inspector, ServerStateFile $serverStateFile): int
    {
        self::$options = $this->options();

        return 0;
    }

    public static function recorded(string $name): mixed
    {
        return self::$options[$name] ?? null;
    }
}

final class InspectableStartPamCommand extends StartPamCommand
{
    /** @param array<string, mixed> $options */
    public function useOptions(array $options): void
    {
        $this->input = new \Symfony\Component\Console\Input\ArrayInput($options, $this->getDefinition());
    }

    /** @return list<string> */
    public function commandLine(string $pamBinary): array
    {
        return $this->serverCommand($pamBinary);
    }
}
