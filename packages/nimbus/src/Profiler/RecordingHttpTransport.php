<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Nimbus\Http\Client\ConnectionException;
use Nimbus\Http\Client\OutgoingRequest;
use Nimbus\Http\Client\Response;
use Nimbus\Http\Client\Transport;

/**
 * Records outgoing HTTP client calls (method, URL, status, duration) in the debug panel.
 *
 * @internal Created through DebugPanel.
 */
final readonly class RecordingHttpTransport implements Transport
{
    public function __construct(private Transport $inner, private DebugRecorder $recorder)
    {
    }

    public function send(OutgoingRequest $request): Response
    {
        $started = hrtime(true);
        try {
            $response = $this->inner->send($request);
        } catch (ConnectionException $failure) {
            $this->record($request, $failure, $started);
            throw $failure;
        }
        $this->record($request, $response, $started);
        return $response;
    }

    public function sendConcurrently(array $requests): array
    {
        $started = hrtime(true);
        $responses = $this->inner->sendConcurrently($requests);
        foreach ($responses as $key => $outcome) {
            $this->record($requests[$key], $outcome, $started);
        }
        return $responses;
    }

    private function record(OutgoingRequest $request, Response|ConnectionException $outcome, int $started): void
    {
        $this->recorder->http([
            'method' => $request->method,
            'url' => preg_replace('/([?&](?:[^=&]*(?:token|key|secret|signature)[^=&]*)=)[^&]*/i', '$1********', $request->url),
            'status' => $outcome instanceof Response ? $outcome->status() : null,
            'error' => $outcome instanceof ConnectionException ? $outcome->getMessage() : null,
            'duration_ms' => round((hrtime(true) - $started) / 1_000_000, 3),
        ]);
    }
}
