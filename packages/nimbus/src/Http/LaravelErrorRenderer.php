<?php

declare(strict_types=1);

namespace Nimbus\Http;

use Nimbus\Validation\ValidationException;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Laravel's JSON error bodies, for apps whose clients were written against a Laravel API:
 *
 * ```json
 * {"message": "Not Found"}
 * {"message": "The name field is required. (and 1 more error)", "errors": {"name": ["The name field is required."], "email": ["..."]}}
 * ```
 *
 * Unhandled exceptions answer `{"message": "Server Error"}`, as Laravel does with `APP_DEBUG=false`.
 */
final readonly class LaravelErrorRenderer implements ErrorRenderer
{
    public function render(HttpException $error, Request $request, Response $response): Response
    {
        if ($error instanceof ValidationException) {
            return JsonEncoding::respond($response, [
                'message' => self::validationMessage($error->errors),
                'errors' => (object) $error->errors,
            ], $error->status);
        }
        $message = match (true) {
            $error->status >= 500 && $error->problemCode === ProblemCode::InternalError => 'Server Error',
            $error->problemCode === ProblemCode::NotFound && $error->getMessage() === 'Route not found.' => 'Not Found',
            $error->problemCode === ProblemCode::Unauthenticated => 'Unauthenticated.',
            $error->problemCode === ProblemCode::RateLimited => 'Too Many Attempts.',
            $error->problemCode === ProblemCode::CsrfTokenMismatch => 'CSRF token mismatch.',
            $error->getMessage() !== '' => $error->getMessage(),
            default => self::statusText($error->status),
        };
        return JsonEncoding::respond($response, ['message' => $message], $error->status);
    }

    /** @param array<string, list<string>> $errors */
    private static function validationMessage(array $errors): string
    {
        $messages = [];
        foreach ($errors as $fieldErrors) {
            foreach ($fieldErrors as $message) {
                $messages[] = $message;
            }
        }
        if ($messages === []) {
            return 'The given data was invalid.';
        }
        $more = count($messages) - 1;
        return $more === 0
            ? $messages[0]
            : $messages[0] . ' (and ' . $more . ' more error' . ($more === 1 ? '' : 's') . ')';
    }

    private static function statusText(int $status): string
    {
        return match ($status) {
            400 => 'Bad Request',
            401 => 'Unauthenticated.',
            403 => 'Forbidden',
            404 => 'Not Found',
            405 => 'Method Not Allowed',
            409 => 'Conflict',
            413 => 'Content Too Large',
            415 => 'Unsupported Media Type',
            419 => 'CSRF token mismatch.',
            422 => 'Unprocessable Content',
            429 => 'Too Many Attempts.',
            503 => 'Service Unavailable',
            default => $status >= 500 ? 'Server Error' : 'Error',
        };
    }
}
