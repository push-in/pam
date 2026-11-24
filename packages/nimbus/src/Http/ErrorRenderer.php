<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Writes the body of an HTTP error: every `HttpException` (validation 422 included),
 * the router's own 404/405 and the 500 of an unhandled exception without `onError()`.
 *
 * ```php
 * $app->renderErrorsWith(new LaravelErrorRenderer());
 * ```
 */
interface ErrorRenderer
{
    public function render(HttpException $error, Request $request, Response $response): Response;
}
