<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Pam\Http\Request;
use Pam\Http\Response;

/**
 * The default renderer: RFC 9457 `application/problem+json` with the integer Nimbus `code`.
 *
 * ```json
 * {"type": "https://pam.dev/problems/4", "title": "Route not found.", "status": 404, "code": 4}
 * ```
 */
final readonly class ProblemDetailsRenderer implements ErrorRenderer
{
    public function render(HttpException $error, Request $request, Response $response): Response
    {
        return JsonEncoding::respond(
            $response,
            [
                ...$error->details,
                'type' => 'https://pam.dev/problems/' . $error->problemCode->value,
                'title' => $error->getMessage(),
                'status' => $error->status,
                'code' => $error->problemCode->value,
            ],
            $error->status,
            contentType: 'application/problem+json; charset=utf-8',
        );
    }
}
