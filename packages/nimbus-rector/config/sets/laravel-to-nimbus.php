<?php

declare(strict_types=1);

use Nimbus\Rector\Rector\AbortToHttpExceptionRector;
use Nimbus\Rector\Rector\LaravelRequestCallToNimbusRector;
use Nimbus\Rector\Rector\ResponseHelperToResponseParameterRector;
use Nimbus\Rector\Rector\RouteFacadeToRouterRector;
use Rector\Config\RectorConfig;
use Rector\Renaming\Rector\Name\RenameClassRector;

/**
 * Laravel → Nimbus. Only classes whose Nimbus counterpart keeps the methods used by
 * typical application code are renamed; everything else is listed in
 * docs/nimbus/migracao-laravel.md as a manual step.
 */
return static function (RectorConfig $rectorConfig): void {
    $rectorConfig->ruleWithConfiguration(RenameClassRector::class, [
        // HTTP
        'Illuminate\Http\Request' => 'Pam\Http\Request',
        'Illuminate\Http\JsonResponse' => 'Pam\Http\Response',
        'Illuminate\Http\Response' => 'Pam\Http\Response',
        'Illuminate\Foundation\Http\FormRequest' => 'Nimbus\Validation\FormRequest',
        'Illuminate\Http\Resources\Json\JsonResource' => 'Nimbus\Http\JsonResource',
        'Illuminate\Http\Resources\Json\ResourceCollection' => 'Nimbus\Http\ResourceCollection',
        // HTTP client
        'Illuminate\Support\Facades\Http' => 'Nimbus\Http\Client\Http',
        'Illuminate\Http\Client\Response' => 'Nimbus\Http\Client\Response',
        'Illuminate\Http\Client\RequestException' => 'Nimbus\Http\Client\RequestException',
        'Illuminate\Http\Client\ConnectionException' => 'Nimbus\Http\Client\ConnectionException',
        'Illuminate\Http\Client\Pool' => 'Nimbus\Http\Client\Pool',
        // Facades with the same static methods
        'Illuminate\Support\Facades\Cache' => 'Nimbus\Cache\Cache',
        'Illuminate\Support\Facades\Hash' => 'Nimbus\Security\Hash',
    ]);
    $rectorConfig->rules([
        LaravelRequestCallToNimbusRector::class,
        ResponseHelperToResponseParameterRector::class,
        RouteFacadeToRouterRector::class,
        AbortToHttpExceptionRector::class,
    ]);
};
