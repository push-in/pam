<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Tests\Fixtures\LoginController;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(App::class)]
final class ControllerHandlerTest extends TestCase
{
    public function testClassMethodHandlerReceivesConstructorAndMethodInjection(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/{tenant}/login', [LoginController::class, 'onLogin']);

        $request = new Request(
            'POST',
            '/acme/login',
            [],
            ['content-type' => ['application/json']],
            '{"email":"dev@pam.dev"}',
        );

        $export = $app->handle($request, new Response())->export();

        self::assertSame(200, $export['status']);
        self::assertSame(
            ['message' => 'authenticated:dev@pam.dev', 'tenant' => 'acme'],
            json_decode($export['body'], true, 512, JSON_THROW_ON_ERROR),
        );
    }

    public function testMissingControllerMethodFailsDuringRegistration(): void
    {
        $app = new App(discoverPackages: false);

        $this->expectException(\InvalidArgumentException::class);
        $app->post('/login', [LoginController::class, 'missing']);
    }
}

