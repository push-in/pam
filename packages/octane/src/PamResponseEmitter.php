<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Http\Request;
use Laravel\Octane\OctaneResponse;
use Pam\Http\Response as PamResponse;
use Symfony\Component\HttpFoundation\BinaryFileResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpFoundation\StreamedResponse;

final class PamResponseEmitter
{
    private const OUTPUT_CHUNK_BYTES = 64 * 1024;

    public function emit(
        Request $request,
        PamResponse $target,
        OctaneResponse $source,
        ?string $routeTemplate = null,
    ): void {
        if ($routeTemplate !== null) {
            $target->header('x-pam-route-template', $routeTemplate);
        }
        $this->emitResponse($request, $source->response, $target, $source->outputBuffer ?? '');
    }

    public function emitThrowable(
        Request $request,
        PamResponse $target,
        string $message,
        bool $debug,
    ): void {
        $message = $debug ? $message : 'Internal Server Error';

        $this->emitResponse(
            $request,
            new Response($message, Response::HTTP_INTERNAL_SERVER_ERROR, [
                'Content-Type' => 'text/plain; charset=UTF-8',
            ]),
            $target,
        );
    }

    private function emitResponse(
        Request $request,
        Response $source,
        PamResponse $target,
        string $capturedOutput = '',
    ): void {
        // Laravel's Router::toResponse() already prepares every response with
        // this exact request before the Octane worker reaches the client. A
        // second Symfony prepare pass is both redundant and measurable on the
        // minimal JSON hot path.
        if (PHP_SAPI === 'embed'
            && !$source instanceof StreamedResponse
            && !$source instanceof BinaryFileResponse) {
            if ($capturedOutput !== ''
                && !$request->isMethod('HEAD')
                && !in_array($source->getStatusCode(), [204, 304], true)) {
                $content = $source->getContent();
                $source->setContent($capturedOutput.($content === false ? '' : $content));
            }
            // Use the Embed SAPI header/output hooks just like FrankenPHP uses
            // its SAPI. PAM merges the captured status, repeated headers,
            // cookies and body into the native response envelope after the
            // Octane worker returns. This removes a second PHP-level copy loop.
            $source->send(false);

            return;
        }

        $target->status($source->getStatusCode());

        foreach ($source->headers->allPreserveCaseWithoutCookies() as $name => $values) {
            $normalized = strtolower($name);
            if (in_array($normalized, ['connection', 'transfer-encoding'], true)) {
                continue;
            }
            if ($normalized === 'content-length' && $capturedOutput !== '') {
                continue;
            }
            foreach ($values as $value) {
                $target->addHeader($name, $value);
            }
        }
        foreach ($source->headers->getCookies() as $cookie) {
            $target->addHeader('set-cookie', (string) $cookie);
        }

        if ($request->isMethod('HEAD') || in_array($source->getStatusCode(), [204, 304], true)) {
            $target->send('');

            return;
        }

        if ($source instanceof StreamedResponse || $source instanceof BinaryFileResponse) {
            $contentType = $source->headers->get('content-type');
            $target->header(
                'content-type',
                is_string($contentType) && $contentType !== '' ? $contentType : 'application/octet-stream',
            );
            $this->streamContent($source, $target, $capturedOutput);

            return;
        }

        $content = $source->getContent();
        $target->send($capturedOutput.($content === false ? '' : $content));
    }

    private function streamContent(
        StreamedResponse|BinaryFileResponse $response,
        PamResponse $target,
        string $capturedOutput,
    ): void {
        if ($capturedOutput !== '') {
            $target->writeChunk($capturedOutput);
        }

        $outputLevel = ob_get_level();
        try {
            ob_start(
                static function (string $buffer) use ($target): string {
                    $target->writeChunk($buffer);

                    return '';
                },
                self::OUTPUT_CHUNK_BYTES,
            );
            $response->sendContent();
            if (ob_get_level() > $outputLevel) {
                ob_end_flush();
            }
        } finally {
            while (ob_get_level() > $outputLevel) {
                ob_end_clean();
            }
        }
    }
}
