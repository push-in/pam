<?php

use App\Providers\ZygoteProbeServiceProvider;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Redis;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Str;

Route::get('/z/info', function () {
    $raw = ZygoteProbeServiceProvider::$rawSocket;
    $written = $raw === null ? null : @fwrite($raw, "*1\r\n$4\r\nPING\r\n");

    return response()->json([
        'pid' => getmypid(),
        'bootPid' => ZygoteProbeServiceProvider::$bootPid,
        'worker' => getenv('PAM_WORKER_ID') ?: null,
        'forked' => class_exists(\Pam\Fork::class) && \Pam\Fork::isForkedWorker(),
        'mysql' => DB::selectOne('select connection_id() as id')->id,
        'redis' => Redis::connection()->client()->executeRaw(['CLIENT', 'ID']),
        'mt' => mt_rand(),
        'rand' => rand(),
        'uniqid' => uniqid('', true),
        'str' => Str::random(24),
        'int' => random_int(PHP_INT_MIN, PHP_INT_MAX),
        'uuid' => (string) Str::uuid(),
        'raw' => $written === null ? 'none' : (($written === false || $written === 0) ? 'failed' : 'written'),
    ]);
});

Route::middleware('web')->group(function () {
    Route::get('/z/login/{id}', function (int $id) {
        Auth::loginUsingId($id);
        session()->put('owner', $id);
        session()->regenerate();

        return response()->json(['pid' => getmypid(), 'session' => session()->getId()]);
    });
    Route::get('/z/me', function () {
        usleep(random_int(0, 2000));
        session()->put('hits', (int) session('hits', 0) + 1);

        return response()->json([
            'pid' => getmypid(),
            'auth' => Auth::id(),
            'owner' => session('owner'),
            'session' => session()->getId(),
            'hits' => session('hits'),
        ]);
    });
});

Route::get('/z/slow', function () {
    usleep((int) request('ms', 1000) * 1000);

    return response()->json(['pid' => getmypid(), 'slept' => (int) request('ms', 1000)]);
});

// PHP memory_limit fatal: the worker answers 500 and is recycled.
Route::get('/z/oom', function () {
    $chunks = [];
    while (true) {
        $chunks[] = str_repeat('x', 1024 * 1024);
    }
});

// Kernel OOM: unbounded PHP memory inside a memory-capped cgroup.
Route::get('/z/hog', function () {
    ini_set('memory_limit', '-1');
    $chunks = [];
    for ($i = 0; $i < (int) request('mb', 3000); $i++) {
        $chunks[] = str_repeat(chr($i % 256), 1024 * 1024);
    }

    return response()->json(['pid' => getmypid(), 'mb' => count($chunks)]);
});
