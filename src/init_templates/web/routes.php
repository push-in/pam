<?php

declare(strict_types=1);

use App\Controllers\DashboardController;
use App\Controllers\LoginController;
use App\Controllers\PasswordResetController;
use App\Controllers\RegisterController;
use App\Middleware\Authenticate;
use Nimbus\Http\Redirect;
use Nimbus\Middleware\RateLimitMiddleware;
use Nimbus\RouteRegistrar;
use Nimbus\Session\Session;
use Pam\Http\Request;

/** @var RouteRegistrar $router */

// Ten attempts per client, then one per second: login, registration and reset links.
$throttle = new RateLimitMiddleware(requestsPerSecond: 1, burst: 10);

$router->get('/', static fn (): Redirect => Redirect::to('/dashboard'));

$router->get('login', [LoginController::class, 'show']);
$router->post('login', [LoginController::class, 'store'])->middleware($throttle);
$router->post('logout', [LoginController::class, 'destroy']);

$router->get('register', [RegisterController::class, 'show']);
$router->post('register', [RegisterController::class, 'store'])->middleware($throttle);

$router->get('forgot-password', [PasswordResetController::class, 'request']);
$router->post('forgot-password', [PasswordResetController::class, 'email'])->middleware($throttle);
$router->get('reset-password/{token}', [PasswordResetController::class, 'edit']);
$router->post('reset-password', [PasswordResetController::class, 'update'])->middleware($throttle);

$router->get('locale/{locale}', static function (Request $request, Session $session): Redirect {
    $locale = (string) $request->route('locale');
    if (in_array($locale, ['pt_BR', 'en'], true)) {
        $session->put('locale', $locale);
    }
    return Redirect::to('/login');
});

$router->middleware(Authenticate::class)->group(static function (RouteRegistrar $router): void {
    $router->get('dashboard', [DashboardController::class, 'show']);
});
