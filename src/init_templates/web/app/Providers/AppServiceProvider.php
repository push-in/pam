<?php

declare(strict_types=1);

namespace App\Providers;

use App\Auth\Users;
use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\AuthUserProvider;
use Nimbus\Auth\OneTimeTokens;
use Nimbus\Auth\PasswordResets;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Auth\SessionRevocations;
use Nimbus\Container\Container;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Notifications\Mail\MailChannel;
use Nimbus\Notifications\NotificationDispatcher;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Notifications\Notifier;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class AppServiceProvider implements ServiceProviderInterface
{
    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('AppServiceProvider requires Nimbus\\App.');
        }
        $container = $application->container();

        $container->singleton(Users::class);
        $container->singleton(AuthUserProvider::class, Users::class);
        $container->singleton(OneTimeTokens::class, static function (Container $container): OneTimeTokens {
            $databases = $container->get(ArcanoDatabaseProvider::class);
            if (!$databases instanceof ArcanoDatabaseProvider) {
                throw new \LogicException('One-time tokens need the database.');
            }
            return new OneTimeTokens($databases);
        });
        // A password reset signs the user's sessions out on every device.
        $container->singleton(SessionRevocations::class, static function (Container $container): SessionRevocations {
            $tokens = $container->get(OneTimeTokens::class);
            if (!$tokens instanceof OneTimeTokens) {
                throw new \LogicException('Session revocations need one-time tokens.');
            }
            return new SessionRevocations($tokens);
        });
        $container->singleton(SessionAuthenticator::class, static function (Container $container): SessionAuthenticator {
            $revocations = $container->get(SessionRevocations::class);
            return new SessionAuthenticator(
                $container,
                static function (string $id) use ($container): ?\Nimbus\Auth\Principal {
                    $users = $container->get(Users::class);
                    return $users instanceof Users ? $users->find($id) : null;
                },
                $revocations instanceof SessionRevocations ? $revocations : null,
            );
        });
        $container->singleton(Authenticator::class, SessionAuthenticator::class);

        // Mail goes through MAILER_DSN (null://null drops it); tests replace the dispatcher with a fake.
        $container->singleton(NotificationDispatcher::class, static fn (): Notifier => new Notifier(new NotificationSender([
            'mail' => MailChannel::fromEnvironment(),
        ])));
        $container->singleton(PasswordResets::class, static function (Container $container): PasswordResets {
            $users = $container->get(AuthUserProvider::class);
            $tokens = $container->get(OneTimeTokens::class);
            $sessions = $container->get(SessionRevocations::class);
            $notifications = $container->get(NotificationDispatcher::class);
            if (!$users instanceof AuthUserProvider || !$tokens instanceof OneTimeTokens || !$sessions instanceof SessionRevocations || !$notifications instanceof NotificationDispatcher) {
                throw new \LogicException('Password resets need the user provider, the database and a notification dispatcher.');
            }
            $url = rtrim(getenv('APP_URL') ?: 'http://localhost:3000', '/') . '/reset-password/{token}';
            return new PasswordResets($users, $tokens, $notifications, $url, revoke: [$sessions]);
        });
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
