<?php

declare(strict_types=1);

use Illuminate\Support\Facades\Route;

Route::get('/api/ping', static fn (): array => ['message' => 'pong']);
