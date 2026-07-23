<?php

declare(strict_types=1);

require_once __DIR__ . '/../compat/composer-smoke/vendor/autoload.php';

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Socket\Server as SocketServer;
use Pam\WS\Socket;

$app = new App();

$app->get('/api/v1/ping', static fn (Request $request, Response $response) => $response->json([
    'status' => 'success',
    'message' => 'pong',
    'search' => $request->getQuery('search'),
]));

$app->post('/api/v1/echo', static fn (Request $request, Response $response) => $response->json([
    'received' => $request->json(),
]));

$ws = SocketServer::create();
$ws->on('connection', static function (Socket $socket) use ($ws): void {
    $socket->emit('welcome', [
        'message' => 'Conectado ao Pam Socket!',
        'sessionId' => $socket->id,
        'resumeToken' => $socket->resumeToken,
    ]);
    $socket->join('lobby');

    $socket->on('chat_message', static function (array $data) use ($ws, $socket): void {
        $ws->emit('chat_message', [
            'user' => $socket->id,
            'message' => $data['message'] ?? '',
        ]);
    });

    $socket->on('disconnect', static function () use ($socket): void {
        echo "Socket {$socket->id} disconnected\n";
    });
});

$port = (int) (getenv('PAM_PORT') ?: 3000);
$resumeSecret = getenv('PAM_WS_RESUME_SECRET');
$options = is_string($resumeSecret) && $resumeSecret !== ''
    ? ['websocketResumeSecret' => $resumeSecret]
    : [];
$app->listen($port, options: $options);
