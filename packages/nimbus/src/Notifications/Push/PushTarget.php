<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

/**
 * A device token on one platform. Routes may also use the `fcm:<token>` / `apns:<token>` strings.
 */
final readonly class PushTarget
{
    public function __construct(public PushPlatform $platform, public string $token)
    {
        if ($token === '' || strlen($token) > 4_096 || preg_match('/[\s\0]/', $token) === 1) {
            throw new \InvalidArgumentException('Push token is invalid.');
        }
    }

    public static function fcm(string $token): self
    {
        return new self(PushPlatform::Fcm, $token);
    }

    public static function apns(string $token): self
    {
        return new self(PushPlatform::Apns, $token);
    }

    public static function parse(string $target): self
    {
        [$platform, $token] = array_pad(explode(':', $target, 2), 2, '');
        $platform = PushPlatform::tryFrom($platform)
            ?? throw new \InvalidArgumentException('Push targets are written fcm:<token> or apns:<token>.');
        return new self($platform, $token);
    }

    public function toString(): string
    {
        return $this->platform->value . ':' . $this->token;
    }
}
