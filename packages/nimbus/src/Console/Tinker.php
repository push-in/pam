<?php

declare(strict_types=1);

namespace Nimbus\Console;

use Nimbus\App;
use Nimbus\Database\ArcanoManager;
use Psy\Configuration;
use Psy\Shell;

/**
 * The `nimbus tinker` REPL: boots the application and opens a PsySH shell with `$app`,
 * `$container` and, with Arcano, `$arcano` and `$orm` in scope.
 *
 * The application comes from a bootstrap file that returns an `App` (or a closure that
 * builds one) without calling `listen()`: `--bootstrap=path`, else `bootstrap/app.php`
 * or `app.php` in the current directory. Requires psy/psysh.
 */
final class Tinker
{
    public const array BOOTSTRAP_FILES = ['bootstrap/app.php', 'app.php'];

    private function __construct()
    {
    }

    /** Loads and boots the application of the bootstrap file. */
    public static function application(?string $bootstrap, string $root): App
    {
        $file = $bootstrap;
        if ($file === null) {
            foreach (self::BOOTSTRAP_FILES as $candidate) {
                if (is_file($root . '/' . $candidate)) {
                    $file = $root . '/' . $candidate;
                    break;
                }
            }
        }
        if ($file === null) {
            throw new \RuntimeException('No bootstrap file: create bootstrap/app.php returning the App, or pass --bootstrap=path.');
        }
        if (!is_file($file)) {
            throw new \RuntimeException("Bootstrap file {$file} does not exist.");
        }
        $app = (static fn (string $path): mixed => require $path)($file);
        if ($app instanceof \Closure) {
            $app = $app();
        }
        if (!$app instanceof App) {
            throw new \RuntimeException("{$file} must return a Nimbus\\App or a closure that returns one.");
        }
        return $app->boot();
    }

    /** @return array<string, mixed> Variables of the shell scope. */
    public static function scope(App $app): array
    {
        $scope = ['app' => $app, 'container' => $app->container()];
        if ($app->container()->has(ArcanoManager::class)) {
            $arcano = $app->container()->get(ArcanoManager::class);
            if ($arcano instanceof ArcanoManager) {
                $scope['arcano'] = $arcano;
                $scope['orm'] = $arcano->orm();
            }
        }
        return $scope;
    }

    public static function shell(App $app, bool $interactive = true): Shell
    {
        if (!class_exists(Shell::class)) {
            throw new \LogicException('nimbus tinker requires psy/psysh: composer require --dev psy/psysh.');
        }
        $configuration = new Configuration([
            'updateCheck' => 'never',
            'startupMessage' => 'Nimbus tinker: $app, $container' . ($app->container()->has(ArcanoManager::class) ? ', $arcano, $orm' : '') . '.',
            'interactiveMode' => $interactive ? Configuration::INTERACTIVE_MODE_AUTO : Configuration::INTERACTIVE_MODE_DISABLED,
        ]);
        $shell = new Shell($configuration);
        if (!$interactive) {
            $shell->setOutput(new \Symfony\Component\Console\Output\BufferedOutput());
        }
        $shell->setScopeVariables(self::scope($app));
        return $shell;
    }

    /**
     * Runs `tinker [--bootstrap=path] [--execute=code]`; returns the exit code.
     *
     * @param list<string> $arguments The arguments after the command name.
     */
    public static function main(array $arguments, string $root): int
    {
        $bootstrap = null;
        $code = null;
        foreach ($arguments as $argument) {
            if (str_starts_with($argument, '--bootstrap=')) {
                $bootstrap = substr($argument, 12);
            } elseif (str_starts_with($argument, '--execute=')) {
                $code = substr($argument, 10);
            } else {
                fwrite(STDERR, "Unknown tinker option {$argument}.\n");
                return 2;
            }
        }
        $app = self::application($bootstrap, $root);
        if ($code !== null) {
            $shell = self::shell($app, false);
            $result = $shell->execute($code, true);
            if ($result !== null) {
                fwrite(STDOUT, (is_scalar($result) ? var_export($result, true) : get_debug_type($result)) . "\n");
            }
            return 0;
        }
        return self::shell($app)->run();
    }
}
