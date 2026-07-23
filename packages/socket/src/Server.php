<?php

declare(strict_types=1);

namespace Pam\Socket;

use Pam\Contracts\Runtime\RuntimeCompatibility;
use Pam\Native\Capability;
use Pam\WS\Adapter;
use Pam\WS\RoomEmitter;
use Pam\WS\Server as NativeServer;

final readonly class Server
{
    public function __construct(public NativeServer $server)
    {
        RuntimeCompatibility::discover()->assert([Capability::WebSocket]);
    }

    public static function create(): self
    {
        return new self(new NativeServer());
    }

    public function adapter(Adapter $adapter): self
    {
        $this->server->adapter($adapter);
        return $this;
    }

    public function on(string $event, callable $handler): self
    {
        $this->server->on($event, $handler);
        return $this;
    }

    public function auth(callable $authenticator): self
    {
        $this->server->auth($authenticator);
        return $this;
    }

    public function emit(string $event, mixed $data = null): self
    {
        $this->server->emit($event, $data);
        return $this;
    }

    public function to(string $room): RoomEmitter
    {
        return $this->server->to($room);
    }
}
