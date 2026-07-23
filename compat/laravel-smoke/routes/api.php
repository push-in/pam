<?php

declare(strict_types=1);

use Illuminate\Http\Request;
use Illuminate\Http\UploadedFile;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\Bus;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Facades\Schema;
use Illuminate\Support\Facades\Storage;

Route::get('/ping', static fn (): array => ['message' => 'pong']);

Route::get('/packages', static fn (): array => [
    'horizon' => class_exists(\Laravel\Horizon\HorizonServiceProvider::class),
    'inertia' => class_exists(\Inertia\ServiceProvider::class),
    'livewire' => class_exists(\Livewire\LivewireServiceProvider::class),
    'pulse' => class_exists(\Laravel\Pulse\PulseServiceProvider::class),
    'reverb' => class_exists(\Laravel\Reverb\ReverbServiceProvider::class),
    'sanctum' => class_exists(\Laravel\Sanctum\SanctumServiceProvider::class),
    'scout' => class_exists(\Laravel\Scout\ScoutServiceProvider::class),
    'socialite' => class_exists(\Laravel\Socialite\SocialiteServiceProvider::class),
    'telescope' => class_exists(\Laravel\Telescope\TelescopeServiceProvider::class),
]);

Route::get('/storage-path', static fn (): array => [
    'path' => storage_path(),
]);

Route::get('/locale/{locale}', static function (string $locale): array {
    app()->setLocale($locale);

    return [
        'application' => app()->getLocale(),
        'translator' => trans()->getLocale(),
    ];
});

Route::get('/locale', static fn (): array => [
    'application' => app()->getLocale(),
    'translator' => trans()->getLocale(),
]);

Route::get('/socialite-redirect', static function (): array {
    $factoryContract = 'Laravel\\Socialite\\Contracts\\Factory';
    abort_unless(
        interface_exists($factoryContract),
        404,
    );
    config()->set('services.github', [
        'client_id' => 'pam-client',
        'client_secret' => 'pam-secret',
        'redirect' => 'http://127.0.0.1/auth/callback',
    ]);
    $factory = app($factoryContract);
    if (!is_object($factory) || !method_exists($factory, 'driver')) {
        throw new UnexpectedValueException('Socialite factory is unavailable.');
    }
    $provider = $factory->driver('github');
    if (
        !is_object($provider)
        || !method_exists($provider, 'stateless')
        || !method_exists($provider, 'redirect')
    ) {
        throw new UnexpectedValueException('Socialite provider is invalid.');
    }
    $provider->stateless();
    $response = $provider->redirect();
    if (!is_object($response) || !method_exists($response, 'getTargetUrl')) {
        throw new UnexpectedValueException('Socialite redirect response is invalid.');
    }
    $redirect = $response->getTargetUrl();
    if (!is_string($redirect)) {
        throw new UnexpectedValueException('Socialite redirect target is invalid.');
    }

    return ['redirect' => $redirect];
});

Route::post('/sanctum/token', static function (): array {
    PamLaravelCompatibilitySchema::ensure();
    $user = PamLaravelUser::query()->firstOrCreate(
        ['email' => 'pam@example.test'],
        [
            'name' => 'PAM Laravel',
            'password' => password_hash('compatibility', PASSWORD_ARGON2ID),
        ],
    );

    return ['token' => $user->createToken('pam-compatibility')->plainTextToken];
});

Route::middleware('auth:sanctum')->get('/sanctum/user', static function (Request $request): array {
    $user = $request->user();
    if (!$user instanceof PamLaravelUser) {
        throw new UnexpectedValueException('Sanctum returned an unexpected user type.');
    }
    $email = $user->getAttribute('email');
    if (!is_string($email)) {
        throw new UnexpectedValueException('Sanctum user email is invalid.');
    }

    return [
        'id' => $user->getAuthIdentifier(),
        'email' => $email,
    ];
});

Route::get('/scout/{term}', static function (string $term): array {
    PamLaravelCompatibilitySchema::ensure();

    return [
        'emails' => PamLaravelUser::search($term)->get()->pluck('email')->values()->all(),
    ];
});

Route::post('/echo', static fn (Request $request): array => [
    'value' => $request->input('value'),
    'header' => $request->header('x-pam-test'),
]);

Route::post('/validate', static function (Request $request): array {
    $validated = $request->validate([
        'email' => ['required', 'email'],
        'count' => ['required', 'integer', 'min:1'],
    ]);

    return ['validated' => $validated];
});

Route::post('/upload', static function (Request $request): array {
    $request->validate([
        'description' => ['required', 'string'],
        'document' => ['required', 'file', 'mimetypes:text/plain', 'max:64'],
    ]);
    $file = $request->file('document');
    if (!$file instanceof UploadedFile) {
        throw new UnexpectedValueException('Laravel did not normalize the uploaded file.');
    }

    return [
        'description' => $request->string('description')->toString(),
        'name' => $file->getClientOriginalName(),
        'size' => $file->getSize(),
        'contents' => $file->get(),
    ];
});

Route::get('/auth', static function (): array {
    $user = Auth::guard('pam-token')->user();

    return [
        'authenticated' => $user !== null,
        'id' => $user?->getAuthIdentifier(),
    ];
});

Route::put('/cache/{value}', static function (string $value): array {
    Cache::put('pam-laravel-cache', $value, 60);

    return ['value' => Cache::get('pam-laravel-cache')];
});

Route::get('/cache', static fn (): array => [
    'value' => Cache::get('pam-laravel-cache'),
]);

Route::post('/database/{value}', static function (string $value): array {
    if (!Schema::hasTable('pam_items')) {
        Schema::create('pam_items', static function ($table): void {
            $table->id();
            $table->string('value');
            $table->timestamps();
        });
    }

    DB::transaction(static function () use ($value): void {
        DB::table('pam_items')->insert([
            'value' => $value,
            'created_at' => now(),
            'updated_at' => now(),
        ]);
    });

    return [
        'count' => DB::table('pam_items')->count(),
        'latest' => DB::table('pam_items')->latest('id')->value('value'),
    ];
});

Route::post('/sync-job/{value}', static function (string $value): array {
    Bus::dispatchSync(new PamLaravelSyncJob($value));

    return ['value' => Cache::get('pam-sync-job')];
});

Route::get('/container-injection/{value}', static function (string $value): array {
    app()->instance(
        PamLaravelRequestContext::class,
        new PamLaravelRequestContext($value),
    );
    $eventResponses = Event::dispatch(new PamLaravelInjectedEvent());

    return [
        'event' => $eventResponses[0] ?? null,
        'bus' => Bus::dispatchSync(new PamLaravelInjectedSyncJob()),
    ];
});

Route::put('/filesystem/{value}', static function (string $value): array {
    $path = 'pam-compatibility.txt';
    Storage::disk('local')->put($path, $value);
    $contents = Storage::disk('local')->get($path);
    Storage::disk('local')->delete($path);

    return ['contents' => $contents];
});

Route::get('/state/{value}', static function (string $value): array {
    app()->instance('pam.smoke.request-value', $value);

    return ['value' => app('pam.smoke.request-value')];
});

Route::get('/state', static fn (): array => [
    'leaked' => app()->bound('pam.smoke.request-value')
        || app()->bound('pam.smoke.concurrent-value'),
]);

Route::get('/hold/{value}', static function (string $value): array {
    app()->instance('pam.smoke.concurrent-value', $value);
    \Pam\Async\delay(0.25);

    return ['value' => app('pam.smoke.concurrent-value')];
});

Route::get('/stream', static function () {
    return response()->stream(static function (): void {
        echo 'pam-stream-first|';
        ob_flush();
        \Pam\Async\delay(0.25);
        echo str_repeat('s', 128 * 1024);
        echo '|pam-stream-last';
    }, 200, ['content-type' => 'text/plain; charset=utf-8']);
});

Route::get('/stream-unbounded', static function () {
    return response()->stream(static function (): void {
        for ($chunk = 0; $chunk < 2_048; ++$chunk) {
            echo str_repeat('b', 64 * 1024);
            ob_flush();
        }
    }, 200, ['content-type' => 'application/octet-stream']);
});

Route::get('/download', static function () {
    return response()->download(
        base_path('fixtures/download.txt'),
        'pam-download.txt',
        ['content-type' => 'text/plain; charset=utf-8'],
    );
});

Route::get('/oversized', static fn () => response(str_repeat('x', 256 * 1024)));
