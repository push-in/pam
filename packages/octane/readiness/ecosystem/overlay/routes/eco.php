<?php

use App\Events\ReadinessPing;
use App\Jobs\ReadinessJob;
use App\Models\User;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Log;
use Illuminate\Support\Facades\Mail;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Facades\Storage;
use Inertia\Inertia;

Route::middleware('web')->group(function () {
    Route::get('/', fn () => Inertia::render('Home', ['message' => 'Hello from Inertia', 'pid' => getmypid()]));
    Route::get('/about', fn () => Inertia::render('About', ['visits' => session()->increment('visits')]));
    Route::get('/livewire', fn () => view('livewire-page'));
    Route::get('/eco/broadcast/{message}', function (string $message) {
        event(new ReadinessPing($message));

        return ['sent' => $message];
    });
    Route::get('/eco/job/{token}', function (string $token) {
        ReadinessJob::dispatch($token);

        return ['queued' => $token];
    });
    Route::get('/eco/job-status/{token}', fn (string $token) => ['status' => Cache::get('readiness-job:'.$token)]);
    Route::get('/eco/mail', function () {
        Mail::raw('Readiness mail body', fn ($m) => $m->to('ops@example.test')->subject('Readiness '.request('id')));

        return ['mailed' => request('id')];
    });
    Route::get('/eco/schedule', fn () => ['ran' => Cache::get('readiness-schedule')]);
    Route::post('/eco/store', function (Request $request) {
        $path = $request->file('document')->store('readiness', 'public');

        return ['url' => Storage::disk('public')->url($path), 'path' => $path];
    })->withoutMiddleware(\Illuminate\Foundation\Http\Middleware\ValidateCsrfToken::class);
    Route::get('/eco/token', function () {
        $user = User::firstOrCreate(['email' => 'api@example.test'], ['name' => 'API', 'password' => bcrypt('secret')]);

        return ['token' => $user->createToken('readiness')->plainTextToken];
    });
});

Route::middleware(['api', 'auth:sanctum'])->get('/api/user', fn (Request $request) => $request->user()->only('id', 'email'));
