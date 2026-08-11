<?php

declare(strict_types=1);

$directory = $argv[1] ?? null;
if ($directory === null) {
    fwrite(STDERR, "usage: php soak-report.php <results-directory>\n");
    exit(64);
}
$benchmark = json_decode(
    (string) file_get_contents(rtrim($directory, '/').'/soak.json'),
    true,
    flags: JSON_THROW_ON_ERROR,
);
$rows = array_map(
    'str_getcsv',
    file(rtrim($directory, '/').'/resources.pam.csv', FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: [],
);
array_shift($rows);
$rss = array_values(array_filter(array_map(
    static fn (array $row): int => (int) ($row[1] ?? 0),
    $rows,
)));
$baseline = $rss[0] ?? 0;
$final = $rss === [] ? 0 : $rss[array_key_last($rss)];
$peak = $rss === [] ? 0 : max($rss);
$growth = max(0, $final - $baseline);
$maximumGrowth = (int) (getenv('PAM_SOAK_MAX_RSS_GROWTH_BYTES') ?: 64 * 1024 * 1024);
$passed = ($benchmark['errors'] ?? 1) === 0
    && ($benchmark['rps'] ?? 0) > 0
    && count($rss) >= 2
    && $growth <= $maximumGrowth;
$report = [
    'schema_version' => 1,
    'generated_at' => gmdate(DATE_ATOM),
    'requests_per_second' => $benchmark['rps'] ?? 0,
    'errors' => $benchmark['errors'] ?? 0,
    'samples' => count($rss),
    'rss_baseline_bytes' => $baseline,
    'rss_final_bytes' => $final,
    'rss_peak_bytes' => $peak,
    'rss_growth_bytes' => $growth,
    'rss_growth_limit_bytes' => $maximumGrowth,
    'passed' => $passed,
];
$encoded = json_encode($report, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR)."\n";
file_put_contents(rtrim($directory, '/').'/soak-report.json', $encoded);
echo $encoded;
exit($passed ? 0 : 1);
