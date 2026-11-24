<?php

declare(strict_types=1);

/**
 * Throughput of App::handle() with telemetry absent, installed but disabled
 * (the default: no NIMBUS_METRICS, no configured OpenTelemetry SDK) and with
 * Prometheus metrics on. Rounds alternate so CPU frequency drift hits every
 * variant alike. Each median is compared with the median of both controls
 * (two identical applications without telemetry).
 *
 *     php benchmarks/telemetry.php [requests per round] [rounds]
 *     php benchmarks/telemetry.php --assert   # fails above 1% for "disabled"
 */

require dirname(__DIR__) . '/vendor/autoload.php';

use Nimbus\App;
use Nimbus\Metrics\MetricsRegistry;
use Nimbus\Telemetry\Telemetry;
use Pam\Http\Request;
use Pam\Http\Response;

$assert = in_array('--assert', $argv, true);
$numbers = array_values(array_filter(array_slice($argv, 1), static fn (string $value): bool => ctype_digit($value)));
$requests = (int) ($numbers[0] ?? 100_000);
$rounds = (int) ($numbers[1] ?? 7);

foreach (['NIMBUS_METRICS', 'NIMBUS_TRACING', 'OTEL_PHP_AUTOLOAD_ENABLED'] as $variable) {
    putenv($variable);
}

$build = static function (?Telemetry $telemetry): App {
    $app = new App(discoverPackages: false);
    $telemetry?->install($app);
    $app->get('/orders/{id}', static fn (Request $request, Response $response): Response => $response->json(['id' => $request->route('id')]));
    return $app;
};
$variants = [
    'absent' => $build(null),
    'disabled' => $build(Telemetry::fromEnvironment()),
    'metrics' => $build(Telemetry::create(new MetricsRegistry())),
    // A second control measures the noise between two identical applications.
    'absent_again' => $build(null),
];
$request = new Request('GET', '/orders/42', [], [], '');
$_SERVER['REMOTE_ADDR'] = '127.0.0.1';

$samples = array_fill_keys(array_keys($variants), []);
foreach ($variants as $app) {
    for ($i = 0; $i < 2_000; ++$i) {
        $app->handle($request, new Response());
    }
}
for ($round = 0; $round < $rounds; ++$round) {
    foreach ($variants as $name => $app) {
        $started = hrtime(true);
        for ($i = 0; $i < $requests; ++$i) {
            $app->handle($request, new Response());
        }
        $samples[$name][] = $requests / ((hrtime(true) - $started) / 1e9);
    }
}

$median = static function (array $values): float {
    sort($values);
    return (float) $values[intdiv(count($values), 2)];
};
$baseline = $median([...$samples['absent'], ...$samples['absent_again']]);
$report = [];
foreach ($samples as $name => $values) {
    $throughput = $median($values);
    $report[$name] = [
        'requests_per_second' => round($throughput),
        'change_percent' => round(($throughput / $baseline - 1) * 100, 2),
    ];
}
echo json_encode(['requests_per_round' => $requests, 'rounds' => $rounds, 'variants' => $report], JSON_PRETTY_PRINT), "\n";

if ($assert && $report['disabled']['change_percent'] < -1.0) {
    fwrite(STDERR, "Disabled telemetry costs more than 1% of throughput.\n");
    exit(1);
}
