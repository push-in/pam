<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Enums\RemoteAction;
use Pam\Laravel\Enums\RemoteProviderType;
use Pam\Laravel\Support\ConfigValue;
use RuntimeException;

final readonly class RemoteControlClient
{
    /**
     * @param array<string, bool|float|int|string> $parameters
     * @return array<string, mixed>
     */
    public function execute(RemoteAction $action, string $target, array $parameters = []): array
    {
        if (!preg_match('/^[a-z][a-z0-9_-]{0,62}$/', $target)) {
            throw new RuntimeException('Remote target must use lowercase letters, digits, dashes or underscores.');
        }
        $provider = RemoteProviderType::tryFrom(ConfigValue::int("pam.remote.targets.{$target}.provider"));
        if ($provider === null) {
            throw new RuntimeException("Remote target {$target} has no valid provider.");
        }

        return match ($provider) {
            RemoteProviderType::PamCloud => $this->cloud($action, $target, $parameters),
            RemoteProviderType::Forge => $this->forge($action, $target, $parameters),
        };
    }

    /**
     * @param array<string, bool|float|int|string> $parameters
     * @return array<string, mixed>
     */
    private function cloud(RemoteAction $action, string $target, array $parameters): array
    {
        $baseUrl = rtrim(ConfigValue::string('pam.remote.cloud.url'), '/');
        $project = ConfigValue::string('pam.remote.cloud.project');
        $token = ConfigValue::string('pam.remote.cloud.token');
        if ($baseUrl === '' || $project === '' || $token === '') {
            throw new RuntimeException('PAM Cloud requires PAM_CLOUD_URL, PAM_CLOUD_PROJECT and PAM_CLOUD_TOKEN.');
        }
        $url = $baseUrl.'/v1/projects/'.rawurlencode($project).'/environments/'.rawurlencode($target).'/'.$action->path();
        $method = in_array($action, [RemoteAction::Status, RemoteAction::Logs, RemoteAction::Top, RemoteAction::Workers, RemoteAction::Queues, RemoteAction::Scheduler], true)
            ? 'GET'
            : 'POST';
        if ($method === 'GET' && $parameters !== []) {
            $url .= '?'.http_build_query($parameters, '', '&', PHP_QUERY_RFC3986);
        }

        return $this->request($method, $url, [
            'Authorization: Bearer '.$token,
            'Accept: application/json',
            'Content-Type: application/json',
        ], ['action' => $action->value] + $parameters);
    }

    /**
     * @param array<string, bool|float|int|string> $parameters
     * @return array<string, mixed>
     */
    private function forge(RemoteAction $action, string $target, array $parameters): array
    {
        if ($action !== RemoteAction::Deploy) {
            throw new RuntimeException('Forge webhook targets support deploy. Use PAM Cloud for remote rollback, logs, top and scaling.');
        }
        $url = ConfigValue::string("pam.remote.targets.{$target}.webhook");
        if ($url === '') {
            throw new RuntimeException("Forge target {$target} requires a deployment webhook.");
        }

        return $this->request('POST', $url, ['Accept: application/json'], ['action' => $action->value] + $parameters);
    }

    /**
     * @param list<string> $headers
     * @param array<string, bool|float|int|string> $payload
     * @return array<string, mixed>
     */
    private function request(string $method, string $url, array $headers, array $payload): array
    {
        $scheme = strtolower((string) parse_url($url, PHP_URL_SCHEME));
        if ($scheme !== 'https' && !ConfigValue::bool('pam.remote.allow_insecure_http')) {
            throw new RuntimeException('Remote control endpoints must use HTTPS.');
        }
        if (!function_exists('curl_init')) {
            throw new RuntimeException('Remote control requires the PHP cURL extension.');
        }

        $handle = curl_init($url);
        if ($handle === false) {
            throw new RuntimeException('Could not initialize the remote control client.');
        }
        $timeout = max(1, min(120, ConfigValue::int('pam.remote.timeout_seconds', 15)));
        curl_setopt($handle, CURLOPT_HTTPHEADER, $headers);
        curl_setopt($handle, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($handle, CURLOPT_CONNECTTIMEOUT, min(10, $timeout));
        curl_setopt($handle, CURLOPT_TIMEOUT, $timeout);
        if ($method === 'POST') {
            curl_setopt($handle, CURLOPT_POST, true);
            curl_setopt($handle, CURLOPT_POSTFIELDS, json_encode($payload, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES));
        }
        $body = curl_exec($handle);
        $status = (int) curl_getinfo($handle, CURLINFO_RESPONSE_CODE);
        $error = curl_error($handle);
        curl_close($handle);
        if (!is_string($body) || $status < 200 || $status >= 300) {
            throw new RuntimeException("Remote control failed with HTTP {$status}: {$error}");
        }
        if ($body === '') {
            return ['ok' => true, 'status' => $status];
        }
        $decoded = json_decode($body, true, flags: JSON_THROW_ON_ERROR);

        return is_array($decoded) ? $decoded : ['ok' => true, 'status' => $status];
    }
}
