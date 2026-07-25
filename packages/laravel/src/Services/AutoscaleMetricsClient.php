<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Support\ConfigValue;
use RuntimeException;

final readonly class AutoscaleMetricsClient
{
    /** @return array{cpu: float, p95: float} */
    public function read(string $url): array
    {
        $scheme = strtolower((string) parse_url($url, PHP_URL_SCHEME));
        $host = strtolower((string) parse_url($url, PHP_URL_HOST));
        if ($scheme !== 'https' && !($scheme === 'http' && in_array($host, ['127.0.0.1', 'localhost', '::1'], true))) {
            throw new RuntimeException('Autoscaling metrics must use HTTPS or a loopback HTTP endpoint.');
        }
        if (!function_exists('curl_init')) {
            throw new RuntimeException('Autoscaling metrics require the PHP cURL extension.');
        }
        $handle = curl_init($url);
        if ($handle === false) {
            throw new RuntimeException('Could not initialize the autoscaling metrics client.');
        }
        $headers = ['Accept: application/json'];
        $token = ConfigValue::string('pam.autoscaling.metrics_token');
        if ($token !== '') {
            $headers[] = 'Authorization: Bearer '.$token;
        }
        curl_setopt_array($handle, [
            CURLOPT_HTTPHEADER => $headers,
            CURLOPT_RETURNTRANSFER => true,
            CURLOPT_CONNECTTIMEOUT => 3,
            CURLOPT_TIMEOUT => 5,
        ]);
        $body = curl_exec($handle);
        $status = (int) curl_getinfo($handle, CURLINFO_RESPONSE_CODE);
        curl_close($handle);
        if (!is_string($body) || $status < 200 || $status >= 300) {
            throw new RuntimeException("Autoscaling metrics failed with HTTP {$status}.");
        }
        $decoded = json_decode($body, true, flags: JSON_THROW_ON_ERROR);
        $cpu = is_array($decoded) ? ($decoded['cpuPercent'] ?? null) : null;
        $p95 = is_array($decoded) ? ($decoded['p95Milliseconds'] ?? null) : null;
        if (!is_numeric($cpu) || !is_numeric($p95)) {
            throw new RuntimeException('Autoscaling metrics require numeric cpuPercent and p95Milliseconds.');
        }

        return ['cpu' => (float) $cpu, 'p95' => (float) $p95];
    }
}
