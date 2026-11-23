<?php

declare(strict_types=1);

namespace Nimbus\Tests\Console;

use Nimbus\App;
use Nimbus\Console\Tinker;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Tinker::class)]
final class TinkerTest extends TestCase
{
    private string $root;

    protected function setUp(): void
    {
        $this->root = sys_get_temp_dir() . '/nimbus-tinker-' . bin2hex(random_bytes(4));
        mkdir($this->root . '/bootstrap', 0700, true);
        file_put_contents($this->root . '/bootstrap/app.php', <<<'PHP'
            <?php
            $app = new Nimbus\App(discoverPackages: false);
            $app->container()->instance('greeting', 'hello from the app');
            return $app;
            PHP);
        file_put_contents($this->root . '/factory.php', '<?php return static fn () => new Nimbus\App(discoverPackages: false);');
        file_put_contents($this->root . '/wrong.php', '<?php return 42;');
    }

    protected function tearDown(): void
    {
        foreach (['bootstrap/app.php', 'factory.php', 'wrong.php'] as $file) {
            @unlink($this->root . '/' . $file);
        }
        @rmdir($this->root . '/bootstrap');
        @rmdir($this->root);
    }

    public function testItBootsTheApplicationAndExecutesCodeInItsScope(): void
    {
        $app = Tinker::application(null, $this->root);
        self::assertSame('hello from the app', $app->container()->get('greeting'));
        self::assertSame(['app', 'container'], array_keys(Tinker::scope($app)));

        $shell = Tinker::shell($app, false);
        self::assertSame('hello from the app', $shell->execute('$container->get("greeting")', true));
        self::assertSame($app, $shell->execute('$app', true));
    }

    public function testBootstrapFilesMayReturnAFactory(): void
    {
        self::assertInstanceOf(App::class, Tinker::application($this->root . '/factory.php', $this->root));
    }

    public function testInvalidBootstrapsAreReported(): void
    {
        try {
            Tinker::application($this->root . '/wrong.php', $this->root);
            self::fail('A bootstrap without an App should be rejected.');
        } catch (\RuntimeException $expected) {
            self::assertStringContainsString('must return a Nimbus\App', $expected->getMessage());
        }
        $this->expectException(\RuntimeException::class);
        Tinker::application(null, sys_get_temp_dir() . '/nimbus-no-such-dir');
    }

    public function testTheCommandLineExecutesCode(): void
    {
        $output = [];
        // The command finds bootstrap/app.php in the directory it runs in.
        exec(
            'cd ' . escapeshellarg($this->root) . ' && ' . escapeshellarg(PHP_BINARY) . ' '
                . escapeshellarg(dirname(__DIR__, 2) . '/bin/nimbus')
                . ' tinker --execute=' . escapeshellarg('strtoupper($container->get("greeting"))') . ' 2>&1',
            $output,
            $status,
        );
        self::assertSame(0, $status, implode("\n", $output));
        self::assertContains("'HELLO FROM THE APP'", $output);
    }
}
