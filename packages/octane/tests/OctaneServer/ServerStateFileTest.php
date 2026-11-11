<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

use Pam\Octane\Server\ServerStateFile;
use PHPUnit\Framework\TestCase;

final class ServerStateFileTest extends TestCase
{
    public function test_server_state_file_can_be_managed(): void
    {
        $path = sys_get_temp_dir().'/pam-octane-state-'.bin2hex(random_bytes(4)).'.json';
        $stateFile = new ServerStateFile($path);

        self::assertSame($path, $stateFile->path());

        $stateFile->delete();

        self::assertSame(['masterProcessId' => null, 'state' => []], $stateFile->read());

        $stateFile->writeProcessId(1);
        $stateFile->writeState(['name' => 'Taylor']);
        self::assertSame(['masterProcessId' => 1, 'state' => ['name' => 'Taylor']], $stateFile->read());

        self::assertTrue($stateFile->delete());
        self::assertSame(['masterProcessId' => null, 'state' => []], $stateFile->read());
    }

    public function test_a_corrupt_state_file_reads_as_empty(): void
    {
        $path = sys_get_temp_dir().'/pam-octane-state-'.bin2hex(random_bytes(4)).'.json';
        file_put_contents($path, '{"masterProcessId": "1; rm -rf /"');

        try {
            self::assertSame(['masterProcessId' => null, 'state' => []], (new ServerStateFile($path))->read());
        } finally {
            @unlink($path);
        }
    }
}
