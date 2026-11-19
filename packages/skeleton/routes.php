<?php

declare(strict_types=1);

use App\Controllers\PingController;
use App\Controllers\ProductController;
use Nimbus\RouteRegistrar;

/** @var RouteRegistrar $router */

$router->get('api/ping', [PingController::class, 'show']);

$router->controller(ProductController::class, function (RouteRegistrar $router): void {
    $router->get('api/products', 'index');
    $router->post('api/products', 'store');
});
