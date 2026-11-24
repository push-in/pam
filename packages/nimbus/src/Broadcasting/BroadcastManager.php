<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Http\FormData;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Jobs\JobDispatcher;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Broadcasts `ShouldBroadcast` events through a driver and authorizes private and
 * presence channel subscriptions.
 *
 * ```php
 * $broadcasts = new BroadcastManager(PusherBroadcaster::fromEnvironment());
 * $app->container()->instance(BroadcastManager::class, $broadcasts);
 * $broadcasts->channel('orders.{order}', fn (User $user, string $order) => $user->owns((int) $order));
 * $broadcasts->routes($app); // POST /broadcasting/auth for pusher-js / Laravel Echo
 * ```
 */
final class BroadcastManager
{
    /** @var list<array{string, \Closure}> */
    private array $channels = [];

    public function __construct(private Broadcaster $broadcaster)
    {
    }

    public function broadcaster(): Broadcaster
    {
        return $this->broadcaster;
    }

    /** Replaces the driver, e.g. with a BroadcastFake in tests. */
    public function swap(Broadcaster $broadcaster): self
    {
        $this->broadcaster = $broadcaster;
        return $this;
    }

    /**
     * Authorizes subscriptions to private and presence channels whose name (without the
     * `private-`/`presence-` prefix) matches the pattern; `{name}` segments are passed to
     * the callback after the principal. Return true or false, or for presence channels an
     * array with the member's public data.
     *
     * @param \Closure $callback Receives the Principal and the `{name}` values as strings.
     */
    public function channel(string $pattern, \Closure $callback): self
    {
        $regex = preg_replace_callback(
            '/\\\\\{([A-Za-z_][A-Za-z0-9_]*)\\\\\}/',
            static fn (array $match): string => '(?P<' . $match[1] . '>[^.]+)',
            preg_quote($pattern, '#'),
        );
        $this->channels[] = ['#^' . $regex . '$#D', $callback];
        return $this;
    }

    /**
     * Broadcasts the event, or pushes a BroadcastEvent job when a dispatcher is given and the
     * event is not ShouldBroadcastNow.
     */
    public function dispatch(ShouldBroadcast $event, ?JobDispatcher $jobs = null): void
    {
        if (method_exists($event, 'broadcastWhen') && $event->broadcastWhen() !== true) {
            return;
        }
        $job = self::job($event);
        if ($jobs !== null && !$event instanceof ShouldBroadcastNow) {
            $jobs->dispatch($job);
            return;
        }
        $this->broadcaster->broadcast($job->channels, $job->event, $job->payload, $job->exceptSocketId);
    }

    /**
     * Sends an event to channels directly, without an event class.
     *
     * @param Channel|string|list<Channel|string> $channels
     * @param array<array-key, mixed> $payload
     */
    public function send(Channel|string|array $channels, string $event, array $payload = [], ?string $exceptSocketId = null): void
    {
        $this->broadcaster->broadcast(self::names($channels), $event, $payload, $exceptSocketId);
    }

    /**
     * The auth response body for the principal's subscription to the channel.
     *
     * @return array<string, mixed>
     * @throws HttpException 403 when no pattern authorizes the subscription.
     */
    public function authorize(Principal $principal, string $channel, string $socketId): array
    {
        $presence = str_starts_with($channel, 'presence-');
        if (!$presence && !str_starts_with($channel, 'private-')) {
            throw new HttpException(403, ProblemCode::Forbidden, 'Only private and presence channels need authorization.');
        }
        $name = substr($channel, $presence ? 9 : 8);
        foreach ($this->channels as [$regex, $callback]) {
            if (preg_match($regex, $name, $matches) !== 1) {
                continue;
            }
            $parameters = array_values(array_filter($matches, is_string(...), ARRAY_FILTER_USE_KEY));
            $result = $callback($principal, ...$parameters);
            if ($presence) {
                if (!is_array($result)) {
                    break;
                }
                return $this->broadcaster->authorizeSocket(
                    $channel,
                    $socketId,
                    ['user_id' => $principal->identifier(), 'user_info' => $result],
                );
            }
            if ($result !== true && !is_array($result)) {
                break;
            }
            return $this->broadcaster->authorizeSocket($channel, $socketId);
        }
        throw new HttpException(403, ProblemCode::Forbidden, 'This channel subscription is not authorized.');
    }

    /**
     * Registers the channel auth endpoint (`socket_id` and `channel_name`, as a form or JSON).
     * The principal comes from the bound `Auth\Authenticator`.
     */
    public function routes(App $app, string $path = '/broadcasting/auth'): void
    {
        $container = $app->container();
        $app->post($path, function (Request $request, Response $response) use ($container): Response {
            $authenticator = $container->has(Authenticator::class) ? $container->get(Authenticator::class) : null;
            if (!$authenticator instanceof Authenticator) {
                throw new \LogicException('Channel authorization requires a bound Nimbus\\Auth\\Authenticator.');
            }
            $principal = $authenticator->authenticate($request)
                ?? throw new HttpException(401, ProblemCode::Unauthenticated, 'Authentication is required.');
            if (str_starts_with(strtolower($request->getHeader('content-type') ?? ''), 'application/json')) {
                $json = $request->json();
                $input = is_array($json) ? $json : [];
            } else {
                $input = FormData::parse($request);
            }
            $socketId = $input['socket_id'] ?? null;
            $channel = $input['channel_name'] ?? null;
            if (!is_string($socketId) || !is_string($channel) || $socketId === '' || $channel === ''
                || strlen($socketId) > 255 || strlen($channel) > 255
            ) {
                throw new HttpException(422, ProblemCode::ValidationFailed, 'socket_id and channel_name are required.');
            }
            return $response->header('cache-control', 'no-store')->json($this->authorize($principal, $channel, $socketId));
        })->summary('Authorize a private or presence channel subscription')->tags(['broadcasting']);
    }

    public static function job(ShouldBroadcast $event): BroadcastEvent
    {
        $name = method_exists($event, 'broadcastAs') ? $event->broadcastAs() : $event::class;
        $payload = method_exists($event, 'broadcastWith') ? $event->broadcastWith() : get_object_vars($event);
        $socket = property_exists($event, 'socket') && is_string($event->socket) ? $event->socket : null;
        if (!is_string($name) || $name === '' || !is_array($payload)) {
            throw new \UnexpectedValueException($event::class . ' must broadcast a non-empty name and an array payload.');
        }
        unset($payload['socket']);
        return new BroadcastEvent(self::names($event->broadcastOn()), $name, $payload, $socket);
    }

    /**
     * @param Channel|string|list<Channel|string> $channels
     * @return non-empty-list<string>
     */
    private static function names(Channel|string|array $channels): array
    {
        $names = [];
        foreach (is_array($channels) ? $channels : [$channels] as $channel) {
            $names[] = $channel instanceof Channel ? $channel->name : (new Channel($channel))->name;
        }
        return $names !== [] ? array_values(array_unique($names)) : throw new \UnexpectedValueException('A broadcast needs at least one channel.');
    }
}
