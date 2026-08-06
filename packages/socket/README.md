# pam/socket

Event-oriented socket APIs for Pam's native WebSocket transport, including
authentication, rooms, broadcasts, acknowledgements and Redis Streams/NATS
adapters.

```bash
pam composer require pam/socket
```

```php
use Pam\Socket\Server;

$io = Server::create();
$io->on('connection', static fn ($socket) => $socket->emit('ready'));
```

Pam uses standard RFC 6455 WebSockets; it is not wire-compatible with Engine.IO or
Socket.IO clients.

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.
