<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

use Nimbus\App;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Http\Client\Factory;
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\Transport;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobHandler;
use Pam\Http\Request;
use Pam\Http\Response;
use Symfony\Component\Mailer\Transport\TransportInterface;

/**
 * A development debug panel (in the spirit of Laravel Telescope) over the MemoryProfiler:
 * requests, queries, jobs, exceptions, mails and outgoing HTTP calls of the worker,
 * newest first, at `/_debug` (HTML) and `/_debug/entries` (JSON).
 *
 * ```php
 * $debug = DebugPanel::register($app);              // null in production
 * $app->gate()->define('viewDebugPanel', fn (User $user) => $user->isDeveloper);
 * $jobs = $debug?->jobs($jobs) ?? $jobs;             // record dispatched jobs
 * ```
 *
 * It is off unless APP_ENV (or PAM_ENV) is local, development, dev or testing, or
 * `$enabled` is passed. The routes need an `Auth\Authenticator` and the `viewDebugPanel`
 * ability, which is closed until the application defines it. Entries live in the
 * worker's memory only and secrets in headers and bodies are masked.
 */
final readonly class DebugPanel
{
    private const array DEVELOPMENT_ENVIRONMENTS = ['local', 'development', 'dev', 'testing'];

    private function __construct(private DebugRecorder $recorder, private string $prefix)
    {
    }

    /** @param array<string, string>|null $environment Defaults to getenv(). */
    public static function enabledByDefault(?array $environment = null): bool
    {
        $environment ??= getenv();
        $name = strtolower($environment['APP_ENV'] ?? $environment['PAM_ENV'] ?? '');
        return in_array($name, self::DEVELOPMENT_ENVIRONMENTS, true)
            && !in_array(strtolower($environment['APP_DEBUG'] ?? 'true'), ['0', 'false', 'off', 'no'], true);
    }

    /** Registers the recorder and the routes; null when the panel is disabled. */
    public static function register(App $app, string $prefix = '/_debug', ?bool $enabled = null, int $maximumEntries = 1_000): ?self
    {
        if (!($enabled ?? self::enabledByDefault())) {
            return null;
        }
        $prefix = '/' . trim($prefix, '/');
        $recorder = new DebugRecorder($maximumEntries, [$prefix]);
        $panel = new self($recorder, $prefix);
        $app->observe($recorder);
        $container = $app->container();
        $container->instance(DebugRecorder::class, $recorder)->instance(self::class, $panel);
        if ($container->has(ArcanoDatabaseProvider::class)) {
            $databases = $container->get(ArcanoDatabaseProvider::class);
            if ($databases instanceof ArcanoDatabaseProvider) {
                $databases->onQuery($recorder->query(...));
            }
        }
        $routes = $app->prefix($prefix)->middleware(new DebugPanelMiddleware($container));
        $routes->get('', $panel->page(...));
        $routes->get('entries', $panel->list(...));
        $routes->get('entries/{id}', $panel->show(...));
        return $panel;
    }

    public function recorder(): DebugRecorder
    {
        return $this->recorder;
    }

    /** Records the jobs dispatched through the returned dispatcher. */
    public function jobs(JobDispatcher $jobs): JobDispatcher
    {
        return new RecordingJobDispatcher($jobs, $this->recorder);
    }

    /** Records the jobs a worker runs through the returned handler. */
    public function jobHandler(JobHandler $handler): JobHandler
    {
        return new RecordingJobHandler($handler, $this->recorder);
    }

    /** Records the mails sent through the returned transport (pass it to Mailer or MailChannel). */
    public function mailTransport(TransportInterface $transport): TransportInterface
    {
        return new RecordingMailTransport($transport, $this->recorder);
    }

    /** Records the calls of an HTTP client transport. */
    public function httpTransport(Transport $transport): Transport
    {
        return new RecordingHttpTransport($transport, $this->recorder);
    }

    /** Records the calls made through the `Http` facade of this worker. */
    public function recordHttpFacade(): self
    {
        Http::swap(new Factory($this->httpTransport(Http::factory()->transport())));
        return $this;
    }

    /** @internal */
    public function list(Request $request, Response $response): Response
    {
        [$type, $token, $limit] = $this->filters($request);
        return $response->json(['data' => array_map(
            static fn (DebugEntry $entry): array => $entry->toArray(),
            $this->recorder->entries($type, $token, $limit),
        )]);
    }

    /** @internal */
    public function show(Request $request, Response $response): Response
    {
        $entry = $this->recorder->find($request->route('id') ?? '')
            ?? throw new HttpException(404, ProblemCode::NotFound, 'The debug entry does not exist.');
        $related = $entry->requestToken === null ? [] : $this->recorder->entries(null, $entry->requestToken, 500);
        return $response->json(['data' => $entry->toArray() + ['related' => array_map(
            static fn (DebugEntry $item): array => $item->toArray(),
            array_values(array_filter($related, static fn (DebugEntry $item): bool => $item->id !== $entry->id)),
        )]]);
    }

    /** @internal */
    public function page(Request $request, Response $response): Response
    {
        [$type, $token, $limit] = $this->filters($request);
        $escape = static fn (string $value): string => htmlspecialchars($value, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8');
        $tabs = '<a href="' . $escape($this->prefix) . '"' . ($type === null ? ' class="on"' : '') . '>all</a>';
        foreach (DebugEntryType::cases() as $case) {
            $tabs .= ' <a href="' . $escape($this->prefix . '?type=' . $case->value) . '"'
                . ($type === $case ? ' class="on"' : '') . '>' . $case->value . '</a>';
        }
        $rows = '';
        foreach ($this->recorder->entries($type, $token, $limit) as $entry) {
            $summary = json_encode($entry->data, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_PARTIAL_OUTPUT_ON_ERROR) ?: '';
            $rows .= '<tr><td>' . date('H:i:s', (int) $entry->recordedAt) . '</td><td>' . $entry->type->value . '</td><td>'
                . ($entry->requestToken === null ? '' : '<a href="' . $escape($this->prefix . '?request=' . $entry->requestToken) . '">'
                    . $escape(substr($entry->requestToken, 0, 8)) . '</a>')
                . '</td><td><code>' . $escape(mb_strimwidth($summary, 0, 400, '...')) . '</code></td></tr>';
        }
        $html = '<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Nimbus debug</title><style>'
            . 'body{font:14px system-ui,sans-serif;margin:0;background:#f6f7f9;color:#1d2330}'
            . 'header{background:#1d2330;color:#fff;padding:12px 20px}nav{padding:12px 20px}'
            . 'nav a{margin-right:10px;color:#1d4ed8}nav a.on{font-weight:700;color:#1d2330}'
            . 'table{border-collapse:collapse;width:calc(100% - 40px);margin:0 20px 20px;background:#fff}'
            . 'td,th{border-bottom:1px solid #e3e6ea;padding:6px 8px;text-align:left;vertical-align:top}'
            . 'code{white-space:pre-wrap;word-break:break-all;font-size:12px}</style></head><body>'
            . '<header>Nimbus debug panel: ' . count($this->recorder->entries(null, null, 1_000)) . ' entries in this worker</header>'
            . '<nav>' . $tabs . '</nav><table><thead><tr><th>Time</th><th>Type</th><th>Request</th><th>Data</th></tr></thead><tbody>'
            . ($rows === '' ? '<tr><td colspan="4">No entries yet.</td></tr>' : $rows)
            . '</tbody></table></body></html>';
        return $response->header('content-type', 'text/html; charset=utf-8')->send($html);
    }

    /** @return array{?DebugEntryType, ?string, int} */
    private function filters(Request $request): array
    {
        $query = $request->query();
        $type = is_string($query['type'] ?? null) ? DebugEntryType::tryFrom($query['type']) : null;
        $token = is_string($query['request'] ?? null) && $query['request'] !== '' ? $query['request'] : null;
        $limit = is_numeric($query['limit'] ?? null) ? (int) $query['limit'] : 100;
        return [$type, $token, $limit];
    }
}
