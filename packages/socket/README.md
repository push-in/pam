# pushinbr/pam-socket

Event-oriented socket APIs for Pam's native WebSocket transport, including
authentication, rooms, broadcasts, acknowledgements and Redis Streams/NATS
adapters.

## Start here

PAM Socket is a Composer package for the PAM Runtime; it is not a standalone
WebSocket server. Install PAM first, open your application directory, and add
the package through PAM's Composer toolchain:

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

pam doctor
cd my-app
pam composer require pushinbr/pam-socket
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
