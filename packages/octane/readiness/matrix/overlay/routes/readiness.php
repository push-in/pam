<?php

declare(strict_types=1);

// Readiness probe routes. Loaded by bootstrap/app.php of the disposable test
// application only; never shipped in the package.

use App\Models\User;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\App;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Route;
use Laravel\Octane\Facades\Octane;

if (!class_exists('ReadinessCounter', false)) {
    final class ReadinessCounter
    {
        public int $hits = 0;
    }
}

// Records Octane's WorkerStopping event so the matrix can verify it fires
// when the worker stops (and not while it boots).
\Illuminate\Support\Facades\Event::listen(
    \Laravel\Octane\Events\WorkerStopping::class,
    static function (): void {
        file_put_contents(storage_path('readiness-stopping.log'), getmypid().PHP_EOL, FILE_APPEND);
    },
);

Route::prefix('_r')->group(function (): void {
    Route::get('/hello', fn () => 'Hello World!');
    Route::get('/json', fn () => response()->json(['message' => 'Hello World!', 'n' => 42]));
    Route::get('/db', fn () => response()->json(
        DB::table('users')->where('id', random_int(1, 100))->first(),
    ));
    Route::get('/view', fn () => view('readiness', ['items' => range(1, 20)]));
    // Same page, marked for PAM's edge page cache (headers only elsewhere).
    $cached = Route::get('/view-cached', fn () => view('readiness', ['items' => range(1, 20)]));
    if (class_exists(\Pam\Octane\Http\Middleware\CacheResponse::class)) {
        $cached->middleware(\Pam\Octane\Http\Middleware\CacheResponse::for(ttl: 60, staleWhileRevalidate: 30, tags: ['readiness']));
    }

    Route::get('/pid', fn () => response()->json([
        'pid' => getmypid(),
        'memory' => memory_get_usage(),
        'peak' => memory_get_peak_usage(),
    ]));

    // --- request isolation -------------------------------------------------
    Route::get('/state/write', function (Request $request) {
        // A container singleton bound mid-request must not survive the request.
        App::instance('readiness.leak', $request->query('v', 'leaked'));
        config(['readiness.leak' => $request->query('v', 'leaked')]);
        App::setLocale('pt_BR');
        Auth::login(User::query()->firstOrCreate(
            ['email' => 'state@example.test'],
            ['name' => 'State', 'password' => bcrypt('secret')],
        ));
        session(['readiness' => 'value']);
        request()->attributes->set('readiness', 'value');
        $counter = App::make(ReadinessCounter::class);
        $counter->hits++;
        Cache::store('array')->put('readiness', 'value');

        return response()->json(['ok' => true, 'pid' => getmypid()]);
    })->middleware('web');

    Route::get('/state/read', fn () => response()->json([
        'pid' => getmypid(),
        'instance' => App::bound('readiness.leak') ? App::make('readiness.leak') : null,
        'config' => config('readiness.leak'),
        'locale' => App::getLocale(),
        'auth' => Auth::check(),
        'session' => session('readiness'),
        'attribute' => request()->attributes->get('readiness'),
        'array_cache' => Cache::store('array')->get('readiness'),
        'request_query' => request()->query('v'),
    ]))->middleware('web');

    Route::get('/state/singleton', function () {
        // Container singletons registered at boot are shared by design; this
        // verifies Octane re-resolves the request-bound `request` instance.
        return response()->json([
            'pid' => getmypid(),
            'url' => request()->fullUrl(),
            'header' => request()->header('x-probe'),
        ]);
    });

    // --- Octane features ----------------------------------------------------
    Route::get('/concurrently', function () {
        $started = microtime(true);
        [$a, $b, $c] = Octane::concurrently([
            fn () => 1 + 1,
            fn () => DB::table('users')->count(),
            fn () => strtoupper('pam'),
        ]);

        return response()->json([
            'results' => [$a, $b, $c],
            'ms' => (int) ((microtime(true) - $started) * 1000),
            'dispatcher' => get_class(Octane::tasks()),
        ]);
    });

    Route::get('/octane-cache', function () {
        try {
            $store = Cache::store('octane');
            $store->put('k', 'v', 10);
            $value = $store->get('k');
            $store->increment('counter');

            return response()->json(['ok' => $value === 'v', 'driver' => get_class($store->getStore()), 'counter' => $store->get('counter')]);
        } catch (Throwable $e) {
            return response()->json(['ok' => false, 'error' => $e->getMessage()], 501);
        }
    });

    Route::get('/octane-table', function () {
        try {
            Octane::table('example');

            return response()->json(['ok' => true]);
        } catch (Throwable $e) {
            return response()->json(['ok' => false, 'error' => $e->getMessage()], 501);
        }
    });

    // --- transport ----------------------------------------------------------
    Route::post('/upload', function (Request $request) {
        $file = $request->file('file');
        if ($file === null) {
            return response()->json([
                'ok' => false,
                'content_length' => $request->header('content-length'),
                'files' => array_keys($_FILES),
                'upload_max' => ini_get('upload_max_filesize'),
                'post_max' => ini_get('post_max_size'),
            ], 422);
        }

        $path = $file->getRealPath();
        $size = $file->getSize();
        $moved = null;
        if ($file->isValid() && $request->boolean('move')) {
            // Symfony's move() goes through is_uploaded_file()/move_uploaded_file().
            $target = $file->move(storage_path('app/readiness-uploads'), 'moved.bin');
            $moved = hash_file('sha256', $target->getPathname());
            @unlink($target->getPathname());
        }

        return response()->json([
            'ok' => $file->isValid(),
            'size' => $size,
            'sha256' => $moved ?? hash_file('sha256', $path),
            'error' => $file->getError(),
            'tmp' => $path,
            'moved' => $moved !== null,
            'memory_limit' => ini_get('memory_limit'),
            'peak_memory' => memory_get_peak_usage(true),
            'pid' => getmypid(),
        ]);
    })->withoutMiddleware(\Illuminate\Foundation\Http\Middleware\ValidateCsrfToken::class);

    Route::post('/upload-errors', fn (Request $request) => response()->json([
        'files' => array_map(fn ($file) => $file->getError(), $request->allFiles()),
        'fields' => $request->except([]),
    ]))->withoutMiddleware(\Illuminate\Foundation\Http\Middleware\ValidateCsrfToken::class);

    Route::post('/echo', fn (Request $request) => response()->json([
        'length' => strlen($request->getContent()),
        'sha256' => hash('sha256', $request->getContent()),
        'json' => $request->json()->all(),
    ]));

    Route::get('/stream', fn () => response()->stream(function () {
        foreach (range(1, 5) as $i) {
            echo "chunk-{$i}\n";
            if (ob_get_level() > 0) {
                ob_flush();
            }
            flush();
            usleep(200_000);
        }
    }, 200, ['Content-Type' => 'text/plain']));

    Route::get('/stream-generator', fn () => response()->stream(function () {
        foreach (range(1, 5) as $i) {
            yield "gen-{$i}\n";
            usleep(200_000);
        }
    }, 200, ['Content-Type' => 'text/plain']));

    Route::get('/sse', fn () => response()->eventStream(function () {
        foreach (range(1, 5) as $i) {
            yield ['tick' => $i];
            usleep(200_000);
        }
    }));

    Route::get('/download', function () {
        $path = storage_path('app/readiness-download.bin');
        if (!is_file($path) || filesize($path) !== 50 * 1024 * 1024) {
            $handle = fopen($path, 'wb');
            for ($i = 0; $i < 50; $i++) {
                fwrite($handle, str_repeat(chr(65 + ($i % 26)), 1024 * 1024));
            }
            fclose($handle);
        }

        return response()->download($path, 'readiness.bin');
    });

    Route::get('/stream-download', fn () => response()->streamDownload(function () {
        for ($i = 0; $i < 64; $i++) {
            echo str_repeat('x', 256 * 1024);
        }
    }, 'big.txt'));

    Route::get('/proxy', fn (Request $request) => response()->json([
        'secure' => $request->secure(),
        'scheme' => $request->getScheme(),
        'host' => $request->getHost(),
        'ip' => $request->ip(),
        'url' => url('/x'),
    ]));

    Route::get('/sleep/{ms}', function (int $ms) {
        usleep($ms * 1000);

        return response()->json(['slept' => $ms, 'pid' => getmypid()]);
    });

    // Symfony's Response::send() sends a raw `HTTP/1.1 200 OK` status line.
    Route::get('/send', fn () => response('sent')->send());

    // A request whose upload is on disk when the request timeout cuts it.
    Route::post('/upload-sleep/{ms}', function (int $ms) {
        usleep($ms * 1000);

        return response()->json(['slept' => $ms]);
    });

    Route::get('/crash', function () {
        // Simulates a fatal native crash of the worker process.
        posix_kill(getmypid(), SIGKILL);

        return 'unreachable';
    });

    Route::get('/fatal', function () {
        // Userland fatal error: Octane must answer 500 and keep the worker usable.
        $a = [];
        $a[] = str_repeat('x', 10);
        throw new RuntimeException('readiness-fatal');
    });

    Route::get('/oom', function () {
        // Engine fatal (memory_limit): PHP bails out of the request.
        ini_set('memory_limit', '64M');
        $data = [];
        while (true) {
            $data[] = str_repeat('x', 1024 * 1024);
        }
    });

    Route::get('/exit', function () {
        exit(1);
    });

    Route::get('/watch', fn () => 'watch-v1');

    Route::get('/cookie', fn () => response('ok')
        ->cookie('a', '1')
        ->cookie('b', '2')
        ->header('X-Multi', 'one')
        ->header('X-Multi', 'two', false));

    Route::get('/redirect', fn () => redirect('/_r/hello'));
    Route::get('/empty', fn () => response()->noContent());
    Route::get('/abort', fn () => abort(418));

    // --- edge page cache ----------------------------------------------------
    if (class_exists(\Pam\Octane\Http\Middleware\CacheResponse::class)) {
        $cache = \Pam\Octane\Http\Middleware\CacheResponse::class;
        // Session-backed: StartSession sets a cookie, so PAM must never cache it.
        Route::get('/cache/web', fn () => response()->json([
            'session' => session()->getId(),
            'pid' => getmypid(),
        ]))->middleware(['web', $cache.':60']);
        Route::get('/cache/user/{id}', fn (string $id) => response()->json([
            'name' => DB::table('users')->where('id', (int) $id)->value('name'),
            'pid' => getmypid(),
            'rendered' => microtime(true),
        ]))->middleware($cache::for(ttl: 300, tags: ['users:{id}']));
        Route::get('/cache/rename/{id}', function (string $id) {
            ReadinessCachedUser::query()->findOrFail((int) $id)->update(['name' => request()->query('name')]);

            return response()->json(['ok' => true, 'pid' => getmypid()]);
        });
        Route::get('/cache/purge-all', fn () => response()->json([
            'purged' => \Pam\Octane\Facades\Pam::purgeAll(),
        ]));

        // Cross-user isolation battery: each route echoes who is asking.
        $who = static fn (array $extra = []) => response()->json([
            'cookie' => request()->cookie('uid'),
            'bearer' => request()->bearerToken(),
            'q' => request()->query('u'),
            'pad' => str_repeat('x', 2048),
        ] + $extra);
        Route::get('/cache/me-cookie', fn () => $who())->middleware($cache.':60');
        Route::get('/cache/me-bearer', fn () => $who())->middleware($cache.':60');
        Route::get('/cache/me-query', fn () => $who())->middleware($cache.':60');
        Route::get('/cache/me-vary', fn () => $who()->header('Vary', 'Cookie, Authorization'))
            ->middleware($cache::for(ttl: 60, staleWhileRevalidate: 30));
        Route::get('/cache/me-swr', fn () => $who()->header('Vary', 'Cookie, Authorization'))
            ->middleware($cache::for(ttl: 1, staleWhileRevalidate: 30));
        Route::get('/cache/login', function () {
            session()->put('uid', request()->query('u'));

            return response()->json(['ok' => true]);
        })->middleware('web');
        Route::get('/cache/me-session', fn () => response()->json(['session' => session('uid')]))
            ->middleware(['web', $cache.':60']);
    }
});

if (trait_exists(\Pam\Octane\Cache\PurgesPageCache::class) && !class_exists('ReadinessCachedUser', false)) {
    final class ReadinessCachedUser extends \Illuminate\Database\Eloquent\Model
    {
        use \Pam\Octane\Cache\PurgesPageCache;

        protected $table = 'users';

        protected $guarded = [];
    }
}

// --- Swoole-parity features on PAM (shared tables, shared cache, task
// workers, ticks). `Octane::table()` needs an Octane version that knows PAM
// tables; PamOctane::table() works with every Octane release.
if (!function_exists('readiness_table')) {
    function readiness_table(string $name): object
    {
        try {
            return Octane::table($name);
        } catch (Throwable $error) {
            if (class_exists(\Pam\Octane\PamOctane::class)) {
                return \Pam\Octane\PamOctane::table($name);
            }
            throw $error;
        }
    }
}

// Registered in every worker; only task workers receive ticks.
Octane::tick('readiness', fn () => Cache::store('octane')->increment('readiness-ticks'))->seconds(1);
try {
    Cache::store('octane')->interval('readiness-interval', fn () => \Illuminate\Support\Str::random(16), 1);
} catch (Throwable) {
}

Route::prefix('_r/parity')->group(function (): void {
    Route::get('/table/set', function (Request $request) {
        $ok = readiness_table('example')->set((string) $request->query('key'), [
            'name' => (string) $request->query('name', ''),
            'votes' => (int) $request->query('votes', 0),
        ]);

        return response()->json(['ok' => $ok, 'pid' => getmypid()]);
    });
    Route::get('/table/get', fn (Request $request) => response()->json([
        'row' => readiness_table('example')->get((string) $request->query('key')),
        'pid' => getmypid(),
    ]));
    Route::get('/table/incr', fn (Request $request) => response()->json([
        'votes' => readiness_table('example')->incr((string) $request->query('key'), 'votes'),
        'pid' => getmypid(),
    ]));
    Route::get('/table/stats', fn () => response()->json(readiness_table('example')->stats()));
    Route::get('/cache/remember', fn (Request $request) => response()->json([
        'length' => strlen(Cache::store('octane')->remember(
            'remember-'.$request->query('key'),
            60,
            fn () => str_repeat('x', (int) $request->query('bytes', 512)),
        )),
    ]));
    Route::get('/table/count', fn () => response()->json([
        'count' => readiness_table('example')->count(),
        'class' => get_class(readiness_table('example')),
    ]));
    Route::get('/cache/put', function (Request $request) {
        Cache::store('octane')->put((string) $request->query('key'), (string) $request->query('value'), 60);

        return response()->json(['pid' => getmypid(), 'driver' => get_class(Cache::store('octane')->getStore())]);
    });
    Route::get('/cache/get', fn (Request $request) => response()->json([
        'value' => Cache::store('octane')->get((string) $request->query('key')),
        'pid' => getmypid(),
    ]));
    Route::get('/cache/incr', fn (Request $request) => response()->json([
        'value' => Cache::store('octane')->increment((string) $request->query('key')),
        'pid' => getmypid(),
    ]));
    Route::get('/interval', fn () => response()->json([
        'value' => Cache::store('octane')->get('readiness-interval'),
    ]));
    Route::get('/ticks', fn () => response()->json([
        'ticks' => Cache::store('octane')->get('readiness-ticks'),
    ]));
    Route::get('/concurrently', function (Request $request) {
        $sleep = (int) $request->query('sleep', 200);
        $count = (int) $request->query('count', 4);
        $started = microtime(true);
        // One closure per line: serializable closures are located by line.
        $task = function () use ($sleep) {
            usleep($sleep * 1000);

            return getmypid();
        };
        $results = Octane::concurrently(array_fill(0, $count, $task), (int) $request->query('wait', 5000));

        return response()->json([
            'pids' => array_values($results),
            'ms' => (int) ((microtime(true) - $started) * 1000),
            'worker' => getmypid(),
            'dispatcher' => get_class(Octane::tasks()),
        ]);
    });
    Route::get('/concurrently/error', function () {
        try {
            Octane::concurrently([fn () => throw new RuntimeException('task failed on purpose')]);
        } catch (Throwable $error) {
            return response()->json(['class' => get_class($error), 'message' => $error->getMessage()]);
        }

        return response()->json(['class' => null]);
    });
    Route::get('/dispatch', function (Request $request) {
        Octane::tasks()->dispatch([fn () => Cache::store('octane')->increment('readiness-dispatched')]);

        return response()->json(['ok' => true]);
    });
    Route::get('/dispatched', fn () => response()->json([
        'value' => Cache::store('octane')->get('readiness-dispatched'),
    ]));
});
