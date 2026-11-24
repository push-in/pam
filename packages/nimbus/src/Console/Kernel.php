<?php

declare(strict_types=1);

namespace Nimbus\Console;

use Nimbus\App;
use Symfony\Component\Console\Application;
use Symfony\Component\Console\Command\Command as SymfonyCommand;
use Symfony\Component\Console\Input\ArrayInput;
use Symfony\Component\Console\Input\StringInput;
use Symfony\Component\Console\Output\BufferedOutput;
use Symfony\Component\Console\Output\OutputInterface;

/**
 * The `bin/nimbus` console: built-in commands (`migrate`, `migrate:status`, `migrate:rollback`,
 * `db:seed`, `schema:cache`, `schema:clear`, `schedule:list`, `tinker`) and the application's
 * commands from `app/Console/Commands` (namespace `App\Console\Commands`) and `App::commands()`.
 *
 * The application comes from `bootstrap/app.php` (or `--bootstrap=path`); commands that do not
 * need it (`list`, `help`) run without one. Requires symfony/console.
 */
final class Kernel
{
    public const string COMMANDS_DIRECTORY = 'app/Console/Commands';

    public const string COMMANDS_NAMESPACE = 'App\\Console\\Commands\\';

    private ?Application $console = null;

    private ?string $lastOutput = null;

    public function __construct(private readonly ?App $app, private readonly string $root)
    {
        if (!class_exists(Application::class)) {
            throw new \LogicException('Nimbus console commands require symfony/console: composer require symfony/console.');
        }
    }

    /**
     * Runs `bin/nimbus <command>`; returns the exit code.
     *
     * @param list<string> $arguments The full argv (script name first).
     */
    public static function main(array $arguments, string $root): int
    {
        $bootstrap = null;
        foreach ($arguments as $index => $argument) {
            if (str_starts_with($argument, '--bootstrap=')) {
                $bootstrap = substr($argument, 12);
                unset($arguments[$index]);
            }
        }
        $arguments = array_values($arguments);
        $command = $arguments[1] ?? 'list';
        if ($command === 'tinker') {
            // PsySH owns the terminal; it keeps its own option parsing.
            return Tinker::main(
                [...($bootstrap !== null ? ['--bootstrap=' . $bootstrap] : []), ...array_slice($arguments, 2)],
                $root,
            );
        }
        $app = null;
        if ($bootstrap !== null || self::bootstrapFile($root) !== null) {
            $app = Tinker::application($bootstrap, $root);
        }
        $kernel = new self($app, $root);
        $console = $kernel->console();
        $console->setAutoExit(false);
        return $console->run(new \Symfony\Component\Console\Input\ArgvInput($arguments));
    }

    /** The Symfony console application with every command registered. */
    public function console(): Application
    {
        if ($this->console !== null) {
            return $this->console;
        }
        $console = new Application('Nimbus', self::version());
        $console->setCatchExceptions(true);
        $commands = [
            new Commands\MigrateCommand(),
            new Commands\MigrateStatusCommand(),
            new Commands\MigrateRollbackCommand(),
            new Commands\SeedCommand(),
            new Commands\SchemaCacheCommand(),
            new Commands\SchemaClearCommand(),
            new Commands\ScheduleListCommand(),
            new Commands\TinkerCommand(),
        ];
        foreach ($this->applicationCommands() as $class) {
            $command = $this->app !== null ? $this->app->container()->get($class) : new $class();
            if (!$command instanceof SymfonyCommand) {
                throw new \UnexpectedValueException("{$class} must extend a console Command.");
            }
            $commands[] = $command;
        }
        foreach ($commands as $command) {
            if ($command instanceof Command) {
                $command->withApp($this->app);
            }
            self::add($console, $command);
        }
        return $this->console = $console;
    }

    /**
     * Runs a command in this process (`Schedule::command()`, tests): `call('buck:sync --force')`
     * or `call('buck:sync', ['account' => '7', '--force' => true])`. The output is kept for `output()`.
     *
     * @param array<string, mixed>|list<string> $parameters
     */
    public function call(string $command, array $parameters = [], ?OutputInterface $output = null): int
    {
        $command = $this->commandName($command);
        $console = $this->console();
        $console->setAutoExit(false);
        $console->setCatchExceptions(false);
        if (array_is_list($parameters)) {
            $line = trim($command . ' ' . implode(' ', array_map(
                static fn (mixed $parameter): string => is_string($parameter) && str_starts_with($parameter, '-')
                    ? $parameter
                    : escapeshellarg(is_scalar($parameter) ? (string) $parameter : ''),
                $parameters,
            )));
            $input = new StringInput($line);
        } else {
            $input = new ArrayInput(['command' => $command, ...$parameters]);
        }
        $buffer = $output === null ? new BufferedOutput() : null;
        try {
            return $console->run($input, $output ?? $buffer);
        } finally {
            $console->setCatchExceptions(true);
            $this->lastOutput = $buffer?->fetch();
        }
    }

    /** The output of the last `call()` that wrote to its own buffer. */
    public function output(): string
    {
        return $this->lastOutput ?? '';
    }

    public function app(): App
    {
        return $this->app ?? throw new \RuntimeException(
            'This command needs the application: create bootstrap/app.php returning the App, or pass --bootstrap=path.',
        );
    }

    public function root(): string
    {
        return $this->root;
    }

    /** @return list<class-string> */
    private function applicationCommands(): array
    {
        $classes = $this->app?->commandClasses() ?? [];
        $directory = $this->root . '/' . self::COMMANDS_DIRECTORY;
        if (is_dir($directory)) {
            $files = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($directory, \FilesystemIterator::SKIP_DOTS));
            $found = [];
            foreach ($files as $file) {
                if (!$file instanceof \SplFileInfo || $file->getExtension() !== 'php') {
                    continue;
                }
                $relative = substr($file->getPathname(), strlen($directory) + 1, -4);
                $found[] = self::COMMANDS_NAMESPACE . str_replace('/', '\\', $relative);
            }
            sort($found);
            foreach ($found as $class) {
                if (class_exists($class) && is_subclass_of($class, SymfonyCommand::class)
                    && !(new \ReflectionClass($class))->isAbstract()
                ) {
                    $classes[] = $class;
                }
            }
        }
        return array_values(array_unique($classes));
    }

    /** A command class name becomes its console name. */
    private function commandName(string $command): string
    {
        if (!class_exists($command) || !is_subclass_of($command, SymfonyCommand::class)) {
            return $command;
        }
        foreach ($this->console()->all() as $name => $registered) {
            if ($registered instanceof $command) {
                return $name;
            }
        }
        throw new \InvalidArgumentException("Command {$command} is not registered.");
    }

    private static function add(Application $console, SymfonyCommand $command): void
    {
        $console->addCommand($command);
    }

    private static function bootstrapFile(string $root): ?string
    {
        foreach (Tinker::BOOTSTRAP_FILES as $candidate) {
            if (is_file($root . '/' . $candidate)) {
                return $root . '/' . $candidate;
            }
        }
        return null;
    }

    private static function version(): string
    {
        if (class_exists(\Composer\InstalledVersions::class) && \Composer\InstalledVersions::isInstalled('pushinbr/nimbus')) {
            return \Composer\InstalledVersions::getPrettyVersion('pushinbr/nimbus') ?? 'dev';
        }
        return 'dev';
    }
}
