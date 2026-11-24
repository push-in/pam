<?php

declare(strict_types=1);

namespace Nimbus\Tests\Console;

use Nimbus\App;
use Nimbus\Console\Command;
use Nimbus\Console\Kernel;
use Nimbus\Console\Signature;
use Nimbus\Database\ArcanoSchemaCompiler;
use Nimbus\Database\ArcanoServiceProvider;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Scheduling\MemorySchedulerLock;
use Nimbus\Scheduling\Schedule;
use Nimbus\Scheduling\Scheduler;
use Nimbus\Tests\Fixtures\Console\GreetCommand;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;
use Symfony\Component\Console\Output\BufferedOutput;

#[CoversClass(Kernel::class)]
#[CoversClass(Command::class)]
#[CoversClass(Signature::class)]
#[CoversClass(Schedule::class)]
final class KernelTest extends TestCase
{
    private string $root = '';

    protected function setUp(): void
    {
        if (!class_exists(\Symfony\Component\Console\Application::class)) {
            self::markTestSkipped('symfony/console is not installed.');
        }
        $this->root = sys_get_temp_dir() . '/nimbus-console-' . bin2hex(random_bytes(6));
        mkdir($this->root . '/app/Console/Commands/Reports', 0775, true);
    }

    protected function tearDown(): void
    {
        if ($this->root === '' || !is_dir($this->root)) {
            return;
        }
        $files = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($this->root, \FilesystemIterator::SKIP_DOTS),
            \RecursiveIteratorIterator::CHILD_FIRST,
        );
        foreach ($files as $file) {
            if ($file instanceof \SplFileInfo) {
                $file->isDir() ? rmdir($file->getPathname()) : unlink($file->getPathname());
            }
        }
        rmdir($this->root);
    }

    public function testLaravelSignaturesBecomeSymfonyDefinitions(): void
    {
        $definition = (new GreetCommand())->getDefinition();

        self::assertSame('app:greet', (new GreetCommand())->getName());
        self::assertSame('Greets someone.', (new GreetCommand())->getDescription());
        self::assertTrue($definition->getArgument('name')->isRequired());
        self::assertSame('Who to greet', $definition->getArgument('name')->getDescription());
        self::assertSame('1', $definition->getArgument('times')->getDefault());
        self::assertFalse($definition->getOption('shout')->acceptValue());
        self::assertSame('P', $definition->getOption('prefix')->getShortcut());
        self::assertSame('Hello', $definition->getOption('prefix')->getDefault());
        self::assertTrue($definition->getOption('tag')->isArray());

        [$name, $arguments] = Signature::parse('x:y {ids?*} {mode?}');
        self::assertSame('x:y', $name);
        self::assertTrue($arguments[0]->isArray());
        self::assertFalse($arguments[1]->isRequired());
    }

    public function testCommandsRunWithContainerInjectionAndOutput(): void
    {
        $app = (new App(discoverPackages: false))->commands([GreetCommand::class]);
        $kernel = new Kernel($app, $this->root);

        self::assertSame(0, $kernel->call('app:greet Ada 2 --shout -PHi --tag=a --tag=b'));
        self::assertSame("HI, ADA!\nHI, ADA!\ntags: a,b\n", $kernel->output());
        self::assertSame(0, $kernel->call(GreetCommand::class, ['name' => 'Grace']));
        self::assertSame("Hello, Grace!\n", $kernel->output());
        self::assertSame(1, $kernel->call('app:greet', ['fail']));

        $output = new BufferedOutput();
        $kernel->call('app:greet', ['Linus'], $output);
        self::assertSame("Hello, Linus!\n", $output->fetch());
    }

    public function testCommandsAreDiscoveredInTheApplicationDirectory(): void
    {
        $suffix = bin2hex(random_bytes(4));
        $class = 'Report' . $suffix . 'Command';
        file_put_contents($this->root . '/app/Console/Commands/Reports/' . $class . '.php', <<<PHP
            <?php
            namespace App\\Console\\Commands\\Reports;
            final class {$class} extends \\Nimbus\\Console\\Command
            {
                protected \$signature = 'report:{$suffix}';
                public function handle(): void
                {
                    \$this->line('report ran');
                }
            }
            PHP);
        $loader = function (string $name) use ($class): void {
            if ($name === 'App\\Console\\Commands\\Reports\\' . $class) {
                require $this->root . '/app/Console/Commands/Reports/' . $class . '.php';
            }
        };
        spl_autoload_register($loader);
        try {
            $kernel = new Kernel(new App(discoverPackages: false), $this->root);
            self::assertTrue($kernel->console()->has('report:' . $suffix));
            self::assertTrue($kernel->console()->has('migrate'));
            self::assertSame(0, $kernel->call('report:' . $suffix));
            self::assertSame("report ran\n", $kernel->output());
        } finally {
            spl_autoload_unregister($loader);
        }
    }

    public function testSchemaAndMigrationCommandsUseTheApplicationProviders(): void
    {
        if (!class_exists(\Arcano\ORM\ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $cache = $this->root . '/var/cache/arcano-schema.php';
        $app = new App(discoverPackages: false);
        $app->provider(new ArcanoServiceProvider(
            new DatabaseConfig('default', ['default' => ['driver' => 'sqlite', 'database' => $this->root . '/db.sqlite']]),
            new ArcanoSchemaCompiler(dirname(__DIR__) . '/Fixtures/Arcano', $cache, refresh: false),
            migrations: dirname(__DIR__) . '/Fixtures/migrations',
        ));
        $app->boot();
        $kernel = new Kernel($app, $this->root);

        self::assertSame(0, $kernel->call('schema:clear'));
        self::assertFileDoesNotExist($cache);
        self::assertSame(0, $kernel->call('schema:cache'));
        self::assertFileExists($cache);
        self::assertStringContainsString('Schema cached:', $kernel->output());

        self::assertSame(0, $kernel->call('migrate:status'));
        self::assertStringContainsString('Pending', $kernel->output());
        self::assertSame(0, $kernel->call('migrate', ['--force' => true]));
        self::assertStringContainsString('Migrated:', $kernel->output());
        self::assertSame(0, $kernel->call('migrate'));
        self::assertStringContainsString('already up to date', $kernel->output());
        self::assertSame(0, $kernel->call('migrate:rollback'));
        self::assertStringContainsString('Rolled back:', $kernel->output());
        self::assertSame(1, $kernel->call('db:seed', ['--class' => 'Missing\\Seeder']));
    }

    public function testCommandsThatNeedTheApplicationExplainHowToProvideIt(): void
    {
        $kernel = new Kernel(null, $this->root);
        self::assertTrue($kernel->console()->has('schema:cache'));

        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessage('bootstrap/app.php');
        $kernel->call('migrate');
    }

    public function testTheScheduleRunsCommandsAndFailsOnNonZeroExitCodes(): void
    {
        $app = (new App(discoverPackages: false))->commands([GreetCommand::class]);
        $schedule = new Schedule(container: $app->container());
        $schedule->command('app:greet Ada')->everyMinute()->withoutOverlapping();
        $schedule->command(GreetCommand::class, ['fail'])->everyMinute();
        $failures = [];
        $scheduler = new Scheduler(
            $schedule,
            new MemorySchedulerLock(),
            onFailure: static function (mixed $task, \Throwable $error) use (&$failures): void {
                $failures[] = $error->getMessage();
            },
        );

        $ran = $scheduler->runDue((new \DateTimeImmutable('2026-11-23 10:00:00'))->getTimestamp());

        self::assertSame(['command:app:greet Ada', 'command:' . GreetCommand::class . ' fail'], $ran);
        self::assertCount(1, $failures);
        self::assertStringContainsString('exited with 1', $failures[0]);
        self::assertSame(['command:app:greet Ada', 'command:' . GreetCommand::class . ' fail'], array_map(
            static fn ($task): string => $task->getName(),
            $schedule->tasks(),
        ));
    }

    public function testTheBinaryListsTheBuiltInCommands(): void
    {
        file_put_contents($this->root . '/app.php', "<?php\nreturn new Nimbus\\App(discoverPackages: false);\n");
        exec(
            'cd ' . escapeshellarg($this->root) . ' && ' . escapeshellarg(PHP_BINARY) . ' '
                . escapeshellarg(dirname(__DIR__, 2) . '/bin/nimbus') . ' list 2>&1',
            $output,
            $status,
        );
        self::assertSame(0, $status, implode("\n", $output));
        $listing = implode("\n", $output);
        foreach (['migrate', 'migrate:status', 'db:seed', 'schema:cache', 'schema:clear', 'schedule:list', 'tinker'] as $command) {
            self::assertStringContainsString($command, $listing);
        }
    }
}
