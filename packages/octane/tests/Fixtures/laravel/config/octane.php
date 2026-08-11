<?php

declare(strict_types=1);

use Laravel\Octane\Events\RequestReceived;
use Laravel\Octane\Octane;

return [
    'server' => env('OCTANE_SERVER', 'swoole'),
    'https' => false,
    'listeners' => [
        RequestReceived::class => [
            ...Octane::prepareApplicationForNextOperation(),
            ...Octane::prepareApplicationForNextRequest(),
        ],
    ],
    'warm' => [],
    'flush' => [],
    'tables' => [],
    'cache' => false,
    'watch' => [],
    'max_execution_time' => 0,
    'garbage' => 50,
];
