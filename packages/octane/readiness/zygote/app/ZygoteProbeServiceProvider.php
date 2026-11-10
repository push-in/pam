<?php

namespace App\Providers;

use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Log;
use Illuminate\Support\Facades\Redis;
use Illuminate\Support\ServiceProvider;

class ZygoteProbeServiceProvider extends ServiceProvider
{
    /** @var resource|null A raw socket opened at boot that nobody closes. */
    public static $rawSocket = null;

    public static ?int $bootPid = null;

    public function boot(): void
    {
        self::$bootPid = getmypid();
        if (getenv('ZYGOTE_PROBE_BOOT_IO') !== '1') {
            return;
        }
        // Everything an application commonly does while booting.
        DB::selectOne('select 1');
        Redis::connection()->ping();
        Log::warning('zygote probe booted', ['pid' => getmypid()]);
        mt_rand();
        $socket = stream_socket_client('tcp://127.0.0.1:'.env('REDIS_PORT'), $code, $message, 2.0);
        fwrite($socket, "*3\r\n$6\r\nCLIENT\r\n$7\r\nSETNAME\r\n$10\r\nzygote-raw\r\n");
        fgets($socket);
        self::$rawSocket = $socket;
    }
}
