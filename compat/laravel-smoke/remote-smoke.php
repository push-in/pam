<?php

declare(strict_types=1);

use Illuminate\Contracts\Console\Kernel;
use Pam\Laravel\Enums\RemoteAction;
use Pam\Laravel\Enums\RemoteProviderType;
use Pam\Laravel\Services\RemoteControlClient;

require __DIR__.'/vendor/autoload.php';

$application = require __DIR__.'/bootstrap/app.php';
$application->make(Kernel::class)->bootstrap();

/**
 * @param array<string, bool|float|int|string> $parameters
 * @return array{headers: string, body: string}
 */
function captureRemoteRequest(RemoteAction $action, array $parameters): array
{
    $server = stream_socket_server('tcp://127.0.0.1:0', $errorCode, $errorMessage);
    if ($server === false) {
        throw new RuntimeException("Could not start the remote mock: {$errorCode} {$errorMessage}");
    }
    $address = stream_socket_get_name($server, false);
    $port = is_string($address) ? (int) substr($address, (int) strrpos($address, ':') + 1) : 0;
    $child = pcntl_fork();
    if ($child === -1) {
        throw new RuntimeException('Could not fork the remote client smoke.');
    }
    if ($child === 0) {
        fclose($server);
        try {
            config()->set([
                'pam.remote.allow_insecure_http' => true,
                'pam.remote.cloud.url' => "http://127.0.0.1:{$port}",
                'pam.remote.cloud.project' => 'community',
                'pam.remote.cloud.token' => 'secret',
                'pam.remote.targets.production.provider' => RemoteProviderType::PamCloud->value,
            ]);
            (new RemoteControlClient())->execute($action, 'production', $parameters);
            exit(0);
        } catch (Throwable $exception) {
            fwrite(STDERR, $exception->getMessage().PHP_EOL);
            exit(1);
        }
    }
    $connection = stream_socket_accept($server, 5);
    if ($connection === false) {
        posix_kill($child, SIGTERM);
        throw new RuntimeException('The remote client did not connect.');
    }
    $request = '';
    while (!str_contains($request, "\r\n\r\n")) {
        $chunk = fread($connection, 8192);
        if ($chunk === false || $chunk === '') {
            break;
        }
        $request .= $chunk;
    }
    [$headers, $body] = array_pad(explode("\r\n\r\n", $request, 2), 2, '');
    preg_match('/^Content-Length:\s*(\d+)$/mi', $headers, $lengthMatch);
    $length = (int) ($lengthMatch[1] ?? 0);
    while (strlen($body) < $length) {
        $chunk = fread($connection, $length - strlen($body));
        if ($chunk === false || $chunk === '') {
            break;
        }
        $body .= $chunk;
    }
    fwrite($connection, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
    fclose($connection);
    fclose($server);
    pcntl_waitpid($child, $status);
    if (pcntl_wexitstatus($status) !== 0) {
        throw new RuntimeException('The remote client child failed.');
    }

    return compact('headers', 'body');
}

$logs = captureRemoteRequest(RemoteAction::Logs, ['lines' => 25]);
if (!str_starts_with($logs['headers'], 'GET /v1/projects/community/environments/production/logs?lines=25 HTTP/1.1')) {
    throw new RuntimeException('Remote log query parameters were not encoded.');
}
if (stripos($logs['headers'], 'Authorization: Bearer secret') === false) {
    throw new RuntimeException('Remote bearer authentication was not sent.');
}

$scale = captureRemoteRequest(RemoteAction::Scale, ['process' => 'queue', 'instances' => 2]);
if (!str_starts_with($scale['headers'], 'POST /v1/projects/community/environments/production/scale HTTP/1.1')) {
    throw new RuntimeException('Remote scale did not use the expected endpoint.');
}
$payload = json_decode($scale['body'], true, flags: JSON_THROW_ON_ERROR);
if (($payload['action'] ?? null) !== RemoteAction::Scale->value
    || ($payload['process'] ?? null) !== 'queue'
    || ($payload['instances'] ?? null) !== 2) {
    throw new RuntimeException('Remote scale payload is invalid.');
}

fwrite(STDOUT, "Remote control smoke passed.\n");
