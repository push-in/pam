<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

/**
 * Prometheus metrics of the application, rendered in the text exposition
 * format (0.0.4).
 *
 * Inside a PAM cluster the series live in shared memory
 * ({@see SharedTableMetricStore}), so one scrape of any worker returns the
 * totals of the whole instance. Label values must have bounded cardinality:
 * route templates, never concrete URLs, user ids or query strings.
 */
final class MetricsRegistry
{
    public const int MAXIMUM_BUCKETS = 16;

    private const int MAXIMUM_CACHED_SERIES = 8192;

    /** OpenTelemetry's recommended boundaries for `http.server.request.duration`, in seconds. */
    public const array DEFAULT_BUCKETS = [0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0];

    /** @var array<string, array{type: string, help: string, labels: list<string>, buckets: list<float>, ttl: int}> */
    private array $families = [];

    /** @var array<string, array{help: string, label: ?string, sample: \Closure(): (array<string, int|float>|int|float)}> */
    private array $callbacks = [];

    /** @var array<string, ?string> Store keys of the label sets this process already saw. */
    private array $keys = [];

    public function __construct(private readonly MetricStore $store = new MemoryMetricStore())
    {
    }

    /** Shared by the whole PAM cluster when the runtime provides shared tables, else per process. */
    public static function shared(): self
    {
        return new self(SharedTableMetricStore::available() ? new SharedTableMetricStore() : new MemoryMetricStore());
    }

    /** @param list<string> $labelNames */
    public function counter(string $name, string $help, array $labelNames = []): Counter
    {
        $this->define($name, 'counter', $help, $labelNames);
        return new Counter($this, $name);
    }

    /**
     * A value per series. With `$ttlSeconds`, a series that was not set for that
     * long disappears from the output (a worker that exited).
     *
     * @param list<string> $labelNames
     */
    public function gauge(string $name, string $help, array $labelNames = [], int $ttlSeconds = 0): Gauge
    {
        $this->define($name, 'gauge', $help, $labelNames, [], $ttlSeconds);
        return new Gauge($this, $name);
    }

    /**
     * @param list<string> $labelNames
     * @param list<float> $buckets Upper bounds in increasing order; `+Inf` is implicit.
     */
    public function histogram(
        string $name,
        string $help,
        array $labelNames = [],
        array $buckets = self::DEFAULT_BUCKETS,
    ): Histogram {
        if ($buckets === [] || count($buckets) >= self::MAXIMUM_BUCKETS) {
            throw new \InvalidArgumentException('A histogram needs between 1 and ' . (self::MAXIMUM_BUCKETS - 1) . ' buckets.');
        }
        $sorted = $buckets;
        sort($sorted);
        if ($sorted !== $buckets || count(array_unique($buckets)) !== count($buckets)) {
            throw new \InvalidArgumentException('Histogram buckets must be unique and increasing.');
        }
        $this->define($name, 'histogram', $help, $labelNames, array_map(floatval(...), $buckets));
        return new Histogram($this, $name);
    }

    /**
     * A gauge sampled at scrape time, such as a queue depth. The closure
     * returns one value, or values by the value of `$labelName`.
     *
     * @param \Closure(): (array<string, int|float>|int|float) $sample
     */
    public function callbackGauge(string $name, string $help, \Closure $sample, ?string $labelName = null): self
    {
        self::assertName($name);
        if (isset($this->families[$name]) || isset($this->callbacks[$name])) {
            throw new \InvalidArgumentException("Metric {$name} is already registered.");
        }
        if ($labelName !== null) {
            self::assertName($labelName);
        }
        $this->callbacks[$name] = ['help' => $help, 'label' => $labelName, 'sample' => $sample];
        return $this;
    }

    /** Series of a gauge family set within its TTL. */
    public function freshSeries(string $name): int
    {
        $ttl = $this->families[$name]['ttl'] ?? 0;
        $oldest = time() - $ttl;
        $fresh = 0;
        foreach ($this->store->rows() as $row) {
            if (($row['name'] ?? null) === $name && ($ttl === 0 || (int) ($row['updated'] ?? 0) >= $oldest)) {
                ++$fresh;
            }
        }
        return $fresh;
    }

    /**
     * @internal
     * @param array<string, string> $labels
     */
    public function increment(string $name, array $labels, string $column, int|float $amount): void
    {
        $key = $this->series($name, $labels);
        if ($key !== null) {
            $this->store->add($key, $column, $amount);
        }
    }

    /**
     * @internal
     * @param array<string, string> $labels
     */
    public function set(string $name, array $labels, float $value): void
    {
        $key = $this->series($name, $labels);
        if ($key !== null) {
            $this->store->put($key, ['value' => $value, 'updated' => time()]);
        }
    }

    /**
     * @internal
     * @param array<string, string> $labels
     */
    public function observe(string $name, array $labels, float $value): void
    {
        $key = $this->series($name, $labels);
        if ($key === null) {
            return;
        }
        $bucket = 0;
        foreach ($this->families[$name]['buckets'] as $bound) {
            if ($value <= $bound) {
                break;
            }
            ++$bucket;
        }
        $this->store->add($key, 'b' . $bucket, 1);
        $this->store->add($key, 'sum', $value);
        $this->store->add($key, 'count', 1);
    }

    public function render(): string
    {
        $series = [];
        foreach ($this->store->rows() as $row) {
            $name = $row['name'] ?? '';
            if (is_string($name) && isset($this->families[$name])) {
                $series[$name][] = $row;
            }
        }
        $lines = [];
        $now = time();
        foreach ($this->families as $name => $family) {
            $lines[] = "# HELP {$name} " . self::escapeHelp($family['help']);
            $lines[] = "# TYPE {$name} {$family['type']}";
            $rows = $series[$name] ?? [];
            usort($rows, static fn (array $a, array $b): int => strcmp((string) ($a['labels'] ?? ''), (string) ($b['labels'] ?? '')));
            foreach ($rows as $row) {
                $labels = (string) ($row['labels'] ?? '');
                if ($family['type'] === 'histogram') {
                    array_push($lines, ...self::histogramLines($name, $labels, $family['buckets'], $row));
                    continue;
                }
                if ($family['ttl'] > 0 && (int) ($row['updated'] ?? 0) < $now - $family['ttl']) {
                    continue;
                }
                $value = $row[$family['type'] === 'counter' ? 'sum' : 'value'] ?? 0;
                $lines[] = $name . self::braces($labels) . ' ' . self::number($value);
            }
        }
        foreach ($this->callbacks as $name => $callback) {
            $lines[] = "# HELP {$name} " . self::escapeHelp($callback['help']);
            $lines[] = "# TYPE {$name} gauge";
            try {
                $sample = ($callback['sample'])();
            } catch (\Throwable) {
                continue;
            }
            if (!is_array($sample)) {
                $lines[] = $name . ' ' . self::number($sample);
                continue;
            }
            ksort($sample);
            foreach ($sample as $labelValue => $value) {
                $label = $callback['label'] ?? 'key';
                $lines[] = $name . '{' . $label . '="' . self::escapeLabel((string) $labelValue) . '"} ' . self::number($value);
            }
        }
        return implode("\n", $lines) . "\n";
    }

    /**
     * @param list<string> $labelNames
     * @param list<float> $buckets
     */
    private function define(string $name, string $type, string $help, array $labelNames, array $buckets = [], int $ttl = 0): void
    {
        self::assertName($name);
        if (isset($this->families[$name]) || isset($this->callbacks[$name])) {
            throw new \InvalidArgumentException("Metric {$name} is already registered.");
        }
        foreach ($labelNames as $label) {
            self::assertName($label);
            if ($label === 'le' || str_starts_with($label, '__')) {
                throw new \InvalidArgumentException("Label name {$label} is reserved.");
            }
        }
        $this->families[$name] = [
            'type' => $type,
            'help' => $help,
            'labels' => $labelNames,
            'buckets' => $buckets,
            'ttl' => max(0, $ttl),
        ];
    }

    /** @param array<array-key, mixed> $labels */
    private function series(string $name, array $labels): ?string
    {
        $family = $this->families[$name] ?? throw new \InvalidArgumentException("Metric {$name} is not registered.");
        $values = [];
        foreach ($family['labels'] as $label) {
            $value = $labels[$label] ?? '';
            $values[] = is_scalar($value) ? (string) $value : '';
        }
        $lookup = $name . "\0" . implode("\0", $values);
        if (array_key_exists($lookup, $this->keys)) {
            return $this->keys[$lookup];
        }
        $rendered = [];
        foreach ($family['labels'] as $index => $label) {
            $rendered[] = $label . '="' . self::escapeLabel($values[$index]) . '"';
        }
        $text = implode(',', $rendered);
        $key = strlen($text) > SharedTableMetricStore::LABEL_BYTES ? null : hash('xxh128', $name . "\0" . $text);
        if ($key !== null) {
            $this->store->describe($key, $name, $text);
        }
        if (count($this->keys) < self::MAXIMUM_CACHED_SERIES) {
            $this->keys[$lookup] = $key;
        }
        return $key;
    }

    /**
     * @param list<float> $buckets
     * @param array<string, int|float|string> $row
     * @return list<string>
     */
    private static function histogramLines(string $name, string $labels, array $buckets, array $row): array
    {
        $prefix = $labels === '' ? '' : $labels . ',';
        $lines = [];
        $cumulative = 0;
        foreach ($buckets as $index => $bound) {
            $cumulative += (int) ($row['b' . $index] ?? 0);
            $lines[] = "{$name}_bucket{{$prefix}le=\"" . self::number($bound) . "\"} {$cumulative}";
        }
        $cumulative += (int) ($row['b' . count($buckets)] ?? 0);
        $lines[] = "{$name}_bucket{{$prefix}le=\"+Inf\"} {$cumulative}";
        $lines[] = "{$name}_sum" . self::braces($labels) . ' ' . self::number($row['sum'] ?? 0);
        $lines[] = "{$name}_count" . self::braces($labels) . ' ' . (int) ($row['count'] ?? 0);
        return $lines;
    }

    private static function braces(string $labels): string
    {
        return $labels === '' ? '' : '{' . $labels . '}';
    }

    private static function number(int|float|string $value): string
    {
        if (is_string($value)) {
            $value = (float) $value;
        }
        if (is_int($value)) {
            return (string) $value;
        }
        if (is_nan($value)) {
            return 'NaN';
        }
        if (is_infinite($value)) {
            return $value > 0 ? '+Inf' : '-Inf';
        }
        if ($value === floor($value) && abs($value) < 1e15) {
            return number_format($value, 0, '.', '');
        }
        return var_export($value, true);
    }

    private static function escapeLabel(string $value): string
    {
        return str_replace(['\\', '"', "\n"], ['\\\\', '\\"', '\\n'], $value);
    }

    private static function escapeHelp(string $value): string
    {
        return str_replace(['\\', "\n"], ['\\\\', '\\n'], $value);
    }

    private static function assertName(string $name): void
    {
        if (preg_match('/^[a-zA-Z_][a-zA-Z0-9_]*$/', $name) !== 1) {
            throw new \InvalidArgumentException("Invalid metric or label name: {$name}.");
        }
    }
}
