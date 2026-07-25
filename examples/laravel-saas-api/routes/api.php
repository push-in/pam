<?php

declare(strict_types=1);

use App\Http\Controllers\WorkspaceController;
use Illuminate\Support\Facades\Route;

Route::get('/ping', static fn (): array => ['message' => 'pong']);
Route::apiResource('workspaces', WorkspaceController::class)->only(['index', 'store']);
