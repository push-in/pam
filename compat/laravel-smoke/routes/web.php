<?php

declare(strict_types=1);

use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Facades\Blade;
use Inertia\Inertia;

Route::get('/session', static function (Request $request): array {
    $previous = $request->session()->get('count', 0);
    $count = is_int($previous) ? $previous + 1 : 1;
    $request->session()->put('count', $count);

    return ['count' => $count];
});

Route::get('/csrf', static fn (): array => ['token' => csrf_token()]);

Route::post('/csrf', static fn (): array => ['accepted' => true]);

Route::get('/cookie', static fn (): JsonResponse => response()
    ->json(['cookie' => true])
    ->cookie('pam_laravel', 'compatible', 5, '/', null, false, true, false, 'Lax'));

Route::get('/inertia-contract', static fn () => Inertia::render('Compatibility', [
    'message' => 'inertia-compatible',
]));

Route::get('/livewire-contract', static fn () => response(Blade::render(
    '<livewire:pam-laravel-compatibility />',
)));
