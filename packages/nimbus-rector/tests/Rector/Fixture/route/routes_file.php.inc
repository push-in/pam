<?php

use App\Http\Controllers\OrderController;
use Illuminate\Support\Facades\Route;

Route::get('ping', [OrderController::class, 'ping']);

Route::middleware('auth:sanctum')->prefix('v1')->group(function () {
    Route::get('orders/{id}', [OrderController::class, 'show'])->name('orders.show')->where('id', '[0-9]+');
    Route::post('orders', [OrderController::class, 'store'])->middleware('auth');
    Route::apiResource('products', OrderController::class);
});

Route::prefix('admin')->group(fn () => Route::get('stats', [OrderController::class, 'stats']));

Route::group(['prefix' => 'legacy'], function () {
    Route::get('x', [OrderController::class, 'x']);
});
?>
-----
<?php

use App\Http\Controllers\OrderController;
use Illuminate\Support\Facades\Route;

$router->get('ping', [OrderController::class, 'ping']);

$router->auth()->prefix('v1')->group(function (\Nimbus\RouteRegistrar $router) {
    $router->get('orders/{id}', [OrderController::class, 'show'])->name('orders.show')->where('id', '[0-9]+');
    $router->post('orders', [OrderController::class, 'store'])->auth();
    $router->apiResource('products', OrderController::class);
});

$router->prefix('admin')->group(fn (\Nimbus\RouteRegistrar $router) => $router->get('stats', [OrderController::class, 'stats']));

Route::group(['prefix' => 'legacy'], function () {
    $router->get('x', [OrderController::class, 'x']);
});
?>
