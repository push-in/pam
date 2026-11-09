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
        if (self::insideNativeDispatch()
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
        if ($response instanceof StreamedResponse) {
            $this->expandGeneratorCallback($response);
        }

        $legacyCounter = self::legacyStreamCounter();
        if ($legacyCounter === null) {
            // The runtime owns the output handler and keeps the output-buffer
            // stack intact while a chunk write suspends the request Fiber.
            $target->streamOutput(
                static function () use ($response): void {
                    $response->sendContent();
                },
                self::OUTPUT_CHUNK_BYTES,
            );

            return;
        }

        // PAM <= 2.2.11 only preserves output buffers across a suspended chunk
        // write for its own Laravel bridge, signalled by that bridge's stream
        // counter. Without it the dispatcher tears the buffers down inside the
        // active output handler, which is fatal and poisons the worker.
        $outputLevel = ob_get_level();
        self::adjustLegacyCounter($legacyCounter, 1);
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
            self::adjustLegacyCounter($legacyCounter, -1);
        }
    }

    private static function legacyStreamCounter(): ?\ReflectionProperty
    {
        $factory = 'Pam\\Laravel\\ResponseFactory';
        if (!class_exists($factory, false) || !property_exists($factory, 'activeStreams')) {
            return null;
        }

        return new \ReflectionProperty($factory, 'activeStreams');
    }

    private static function adjustLegacyCounter(\ReflectionProperty $counter, int $delta): void
    {
        $current = $counter->getValue();
        $counter->setValue(null, (is_int($current) ? $current : 0) + $delta);
    }

    /**
     * The SAPI fast path is only valid while PAM is capturing the Embed SAPI
     * output of a native HTTP dispatch. Anywhere else (CLI, tests, embedders)
     * the response must be written to the PAM response object.
     */
    private static function insideNativeDispatch(): bool
    {
        $operation = 'Pam\\Async\\NativeOperation';

        return PHP_SAPI === 'embed'
            && class_exists($operation, false)
            && $operation::available();
    }

    /**
     * Under Octane, Laravel's `response()->stream()` keeps a generator
     * callback untouched and expects the Octane client to iterate it (see
     * Laravel\Octane\StreamsGeneratorResponses). Calling it directly would
     * only create the generator and send an empty body.
     */
    private function expandGeneratorCallback(StreamedResponse $response): void
    {
        $callback = $response->getCallback();
        if (!$callback instanceof \Closure || !(new \ReflectionFunction($callback))->isGenerator()) {
            return;
        }

        $response->setCallback(static function () use ($callback): void {
            foreach ($callback() as $chunk) {
                echo $chunk;
                if (ob_get_level() > 0) {
                    ob_flush();
                }
                if (connection_aborted() === 1) {
                    break;
                }
            }
        });
    }
}
