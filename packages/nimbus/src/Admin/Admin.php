<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Nimbus\App;

/**
 * Entry point of the optional admin panel over Arcano entities.
 *
 * ```php
 * Admin::panel($app, '/admin', loginUrl: '/login')
 *     ->resource(ProductAdmin::class)
 *     ->resource(Category::class);
 *
 * $app->gate()->define('viewAdmin', static fn (User $user): bool => $user->isAdmin);
 * ```
 *
 * Requires `pushinbr/arcano` (ArcanoServiceProvider), the session middleware
 * (SessionServiceProvider, which also checks CSRF) and an `Auth\Authenticator` such as
 * `SessionAuthenticator`.
 */
final class Admin
{
    /**
     * Registers the panel's routes under `$prefix`.
     *
     * @param ?string $loginUrl Where guests are sent (with `?redirect=`); null answers 401.
     * @param ?string $views A directory whose `.nim.php` files replace the built-in templates of the same name.
     */
    public static function panel(
        App $app,
        string $prefix = '/admin',
        ?string $loginUrl = null,
        string $title = 'Admin',
        ?string $views = null,
    ): AdminPanel {
        return new AdminPanel($app, $prefix, $loginUrl, $title, $views);
    }
}
