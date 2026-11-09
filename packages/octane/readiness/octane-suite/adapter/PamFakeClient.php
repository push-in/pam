<?php

declare(strict_types=1);

namespace Laravel\Octane\Tests\Pam;

use Illuminate\Foundation\Application;
use Illuminate\Http\Request;
use Laravel\Octane\OctaneResponse;
use Laravel\Octane\RequestContext;
use Laravel\Octane\Testing\Fakes\FakeClient;
use Pam\Http\Request as PamRequest;
use Pam\Http\Response as PamResponse;
use Pam\Octane\PamClient;
use Symfony\Component\HttpFoundation\BinaryFileResponse;
use Symfony\Component\HttpFoundation\StreamedResponse;
use Throwable;

/**
 * Drop-in replacement for Octane's FakeClient that pushes every request and
 * response of the upstream suite through the real PAM Octane client.
 *
 * Upstream assertions keep running on the original response objects, while
 * each emitted PAM response is compared with what Laravel produced; any
 * difference is recorded and fails the run (see bootstrap.php).
 */
final class PamFakeClient extends FakeClient
{
    /** @var list<string> */
    public static array $mismatches = [];

    public static int $exchanges = 0;

    private PamClient $pam;

    public function __construct(array $requests)
    {
        parent::__construct($requests);
        $this->pam = new PamClient();
    }

    public function marshalRequest(RequestContext $context): array
    {
        /** @var Request $original */
        $original = $context->request;
        // PAM populates PHP's request environment before invoking the bridge.
        $_GET = $original->query->all();
        $_POST = $original->request->all();
        $_COOKIE = $original->cookies->all();
        $_FILES = [];
        $_SERVER = $original->server->all();

        $headers = [];
        foreach ($original->headers->all() as $name => $values) {
            $headers[$name] = array_values(array_map('strval', $values));
        }
        $context->data['pamRequest'] = new PamRequest(
            $original->getMethod(),
            $original->getPathInfo(),
            $original->query->all(),
            $headers,
            (string) $original->getContent(),
        );
        $context->data['pamResponse'] = new PamResponse();

        [$marshaled, $context] = $this->pam->marshalRequest($context);
        foreach ([
            'method' => [$original->getMethod(), $marshaled->getMethod()],
            'uri' => [$original->getRequestUri(), $marshaled->getRequestUri()],
            'host' => [$original->getHttpHost(), $marshaled->getHttpHost()],
            'query' => [$original->query->all(), $marshaled->query->all()],
            'input' => [$original->request->all(), $marshaled->request->all()],
            'cookies' => [$original->cookies->all(), $marshaled->cookies->all()],
        ] as $aspect => [$expected, $actual]) {
            if ($expected !== $actual) {
                self::$mismatches[] = "request {$aspect}: ".json_encode([$expected, $actual]);
            }
        }

        return [$marshaled, $context];
    }

    public function respond(RequestContext $context, OctaneResponse $octaneResponse): void
    {
        parent::respond($context, $octaneResponse);
        ++self::$exchanges;
        $source = $octaneResponse->response;
        if ($source instanceof StreamedResponse || $source instanceof BinaryFileResponse) {
            return; // upstream asserts on the unsent callback; do not consume it here
        }

        $target = $context['pamResponse'];
        $this->pam->respond($context, $octaneResponse);
        $export = $target->export();
        $request = $context['laravelRequest'];
        $bodyless = $request->isMethod('HEAD') || in_array($source->getStatusCode(), [204, 304], true);
        $expected = $bodyless ? '' : ($octaneResponse->outputBuffer ?? '').$source->getContent();
        $actual = $export['body'].implode('', $export['chunks']);
        if ($export['status'] !== $source->getStatusCode() || $actual !== $expected) {
            self::$mismatches[] = sprintf(
                'response %s %s: status %d/%d body %s',
                $request->getMethod(),
                $request->getRequestUri(),
                $source->getStatusCode(),
                $export['status'],
                $actual === $expected ? 'equal' : 'differs',
            );
        }
        foreach ($source->headers->getCookies() as $cookie) {
            if (!in_array((string) $cookie, $export['headers']['set-cookie'] ?? [], true)) {
                self::$mismatches[] = 'cookie lost: '.$cookie->getName();
            }
        }
    }

    public function error(Throwable $e, Application $app, Request $request, RequestContext $context): void
    {
        parent::error($e, $app, $request, $context);
        if (!isset($context['pamResponse'])) {
            return;
        }
        $this->pam->error($e, $app, $request, $context);
        if ($context['pamResponse']->export()['status'] !== 500) {
            self::$mismatches[] = 'error response is not 500';
        }
    }
}
