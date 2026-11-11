<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

use Laravel\Octane\PosixExtension;
use Pam\Octane\Server\ServerProcessInspector;
use Pam\Octane\Server\ServerStateFile;
use PHPUnit\Framework\TestCase;

final class ServerProcessInspectorTest extends TestCase
{
    private string $path;

    protected function setUp(): void
    {
        $this->path = sys_get_temp_dir().'/pam-octane-pid-'.bin2hex(random_bytes(4)).'.json';
    }

    protected function tearDown(): void
    {
        @unlink($this->path);
    }

    public function test_can_determine_if_pam_server_process_is_running_when_master_is_running(): void
    {
        [$inspector, $posix] = $this->inspector(running: true, processId: 1);

        self::assertTrue($inspector->serverIsRunning());
        self::assertSame([[1, 0]], $posix->signals);
    }

    public function test_can_determine_if_pam_server_process_is_running_when_master_cant_be_communicated_with(): void
    {
        [$inspector] = $this->inspector(running: false, processId: 1);

        self::assertFalse($inspector->serverIsRunning());
    }

    public function test_can_determine_if_pam_server_process_is_running_without_a_process_id(): void
    {
        [$inspector, $posix] = $this->inspector(running: true, processId: null);

        self::assertFalse($inspector->serverIsRunning());
        self::assertSame([], $posix->signals);
    }

    public function test_pam_server_process_can_be_reloaded(): void
    {
        [$inspector, $posix] = $this->inspector(running: true, processId: 1);

        $inspector->reloadServer();

        self::assertSame([[1, SIGHUP]], $posix->signals);
    }

    public function test_pam_server_process_can_be_stopped(): void
    {
        [$inspector, $posix] = $this->inspector(running: true, processId: 1);

        self::assertTrue($inspector->stopServer());
        self::assertSame([[1, SIGTERM]], $posix->signals);
    }

    /** @return array{ServerProcessInspector, RecordingPosix} */
    private function inspector(bool $running, ?int $processId): array
    {
        $stateFile = new ServerStateFile($this->path);
        if ($processId !== null) {
            $stateFile->writeProcessId($processId);
        }
        $posix = new RecordingPosix($running);

        return [new ServerProcessInspector($stateFile, $posix), $posix];
    }
}

final class RecordingPosix extends PosixExtension
{
    /** @var list<array{int, int}> */
    public array $signals = [];

    public function __construct(private readonly bool $running)
    {
    }

    public function kill(int $processId, int $signal): bool
    {
        $this->signals[] = [$processId, $signal];

        return $this->running;
    }
}
