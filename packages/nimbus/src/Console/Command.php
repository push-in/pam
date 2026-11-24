<?php

declare(strict_types=1);

namespace Nimbus\Console;

use Nimbus\App;
use Symfony\Component\Console\Command\Command as SymfonyCommand;
use Symfony\Component\Console\Helper\Table;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;

/**
 * An application console command on symfony/console, written as a Laravel one:
 *
 * ```php
 * final class SyncAccounts extends Command
 * {
 *     protected $signature = 'buck:sync {account? : One account id} {--force}';
 *     protected $description = 'Synchronizes the monitored accounts.';
 *
 *     public function handle(AccountSync $sync): int
 *     {
 *         $count = $sync->run($this->argument('account'), (bool) $this->option('force'));
 *         $this->info("Synchronized {$count} accounts.");
 *         return self::SUCCESS;
 *     }
 * }
 * ```
 *
 * `handle()` receives its parameters from the application container. Commands in
 * `app/Console/Commands` are discovered by `bin/nimbus`; others register with
 * `$app->commands([...])`. Requires symfony/console.
 */
abstract class Command extends SymfonyCommand
{
    /**
     * Laravel signature: name, `{argument}`, `{argument?}`, `{--option}`, `{--option=default}`.
     *
     * @var string
     */
    protected $signature = '';

    /** @var string */
    protected $description = '';

    protected ?App $app = null;

    protected InputInterface $input;

    protected SymfonyStyle $output;

    public function __construct()
    {
        if ($this->signature === '') {
            parent::__construct();
        } else {
            [$name, $arguments, $options] = Signature::parse($this->signature);
            parent::__construct($name);
            foreach ($arguments as $argument) {
                $this->getDefinition()->addArgument($argument);
            }
            foreach ($options as $option) {
                $this->getDefinition()->addOption($option);
            }
        }
        if ($this->description !== '') {
            $this->setDescription($this->description);
        }
    }

    /** The application whose container builds `handle()` parameters. */
    public function withApp(?App $app): static
    {
        $this->app = $app;
        return $this;
    }

    /** The application, or a clear error when bin/nimbus found no bootstrap file. */
    protected function app(): App
    {
        return $this->app ?? throw new \RuntimeException(
            'This command needs the application: create bootstrap/app.php returning the App, or pass --bootstrap=path.',
        );
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $this->input = $input;
        $this->output = new SymfonyStyle($input, $output);
        if (!method_exists($this, 'handle')) {
            throw new \LogicException(static::class . ' must define handle().');
        }
        $result = $this->app !== null
            ? $this->app->container()->call([$this, 'handle'])
            : call_user_func([$this, 'handle']);
        return is_int($result) ? $result : self::SUCCESS;
    }

    /** One argument, or every argument without a name. */
    protected function argument(?string $name = null): mixed
    {
        return $name === null ? $this->input->getArguments() : $this->input->getArgument($name);
    }

    /** @return array<string, mixed> */
    protected function arguments(): array
    {
        return $this->input->getArguments();
    }

    protected function option(?string $name = null): mixed
    {
        return $name === null ? $this->input->getOptions() : $this->input->getOption($name);
    }

    /** @return array<string, mixed> */
    protected function options(): array
    {
        return $this->input->getOptions();
    }

    protected function hasOption(string $name): bool
    {
        return $this->input->hasOption($name);
    }

    protected function line(string $message, ?string $style = null): void
    {
        $this->output->writeln($style === null ? $message : "<{$style}>{$message}</{$style}>");
    }

    protected function info(string $message): void
    {
        $this->line($message, 'info');
    }

    protected function comment(string $message): void
    {
        $this->line($message, 'comment');
    }

    protected function warn(string $message): void
    {
        $this->line($message, 'comment');
    }

    protected function error(string $message): void
    {
        $this->line($message, 'error');
    }

    protected function newLine(int $count = 1): void
    {
        $this->output->newLine($count);
    }

    /**
     * @param list<string> $headers
     * @param iterable<array<array-key, mixed>> $rows
     */
    protected function table(array $headers, iterable $rows): void
    {
        $table = new Table($this->output);
        $table->setHeaders($headers);
        foreach ($rows as $row) {
            $table->addRow(array_values($row));
        }
        $table->render();
    }

    protected function ask(string $question, ?string $default = null): mixed
    {
        return $this->output->ask($question, $default);
    }

    protected function confirm(string $question, bool $default = false): bool
    {
        return $this->output->confirm($question, $default);
    }

    /**
     * Runs another command of the application.
     *
     * @param array<string, mixed> $arguments
     */
    protected function call(string $command, array $arguments = []): int
    {
        $application = $this->getApplication() ?? throw new \LogicException('The command is not in a console application.');
        return $application->find($command)->run(
            new \Symfony\Component\Console\Input\ArrayInput(['command' => $command, ...$arguments]),
            $this->output,
        );
    }
}
