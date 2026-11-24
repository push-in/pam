<?php

declare(strict_types=1);

namespace Nimbus\Console;

use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputOption;

/**
 * Parses a Laravel command signature into Symfony arguments and options:
 *
 * `buck:sync {account : The account id} {days?} {--force} {--limit=100} {--Q|queue=*}`
 *
 * @internal Used by `Command`; the signature syntax is the public contract.
 */
final class Signature
{
    /**
     * @return array{string, list<InputArgument>, list<InputOption>}
     */
    public static function parse(string $signature): array
    {
        $signature = trim($signature);
        if (preg_match('/^[^\s{]+/', $signature, $name) !== 1) {
            throw new \InvalidArgumentException('A command signature starts with the command name.');
        }
        preg_match_all('/\{\s*(.*?)\s*\}/s', $signature, $tokens);
        $arguments = [];
        $options = [];
        foreach ($tokens[1] as $token) {
            [$token, $description] = array_pad(array_map(trim(...), explode(' : ', $token, 2)), 2, '');
            if (str_starts_with($token, '--')) {
                $options[] = self::option(substr($token, 2), $description);
            } else {
                $arguments[] = self::argument($token, $description);
            }
        }
        return [$name[0], $arguments, $options];
    }

    private static function argument(string $token, string $description): InputArgument
    {
        return match (true) {
            str_ends_with($token, '?*') => new InputArgument(substr($token, 0, -2), InputArgument::IS_ARRAY, $description),
            str_ends_with($token, '*') => new InputArgument(substr($token, 0, -1), InputArgument::IS_ARRAY | InputArgument::REQUIRED, $description),
            str_ends_with($token, '?') => new InputArgument(substr($token, 0, -1), InputArgument::OPTIONAL, $description),
            preg_match('/(.+)=\*(.+)/', $token, $matches) === 1 => new InputArgument(
                $matches[1],
                InputArgument::IS_ARRAY,
                $description,
                preg_split('/,\s?/', $matches[2]) ?: [],
            ),
            preg_match('/(.+)=(.+)/', $token, $matches) === 1 => new InputArgument($matches[1], InputArgument::OPTIONAL, $description, $matches[2]),
            str_ends_with($token, '=') => new InputArgument(substr($token, 0, -1), InputArgument::OPTIONAL, $description),
            default => new InputArgument($token, InputArgument::REQUIRED, $description),
        };
    }

    private static function option(string $token, string $description): InputOption
    {
        $shortcut = null;
        $parts = preg_split('/\s*\|\s*/', $token, 2) ?: [$token];
        if (count($parts) === 2) {
            [$shortcut, $token] = $parts;
        }
        return match (true) {
            str_ends_with($token, '=') => new InputOption(substr($token, 0, -1), $shortcut, InputOption::VALUE_OPTIONAL, $description),
            str_ends_with($token, '=*') => new InputOption(substr($token, 0, -2), $shortcut, InputOption::VALUE_OPTIONAL | InputOption::VALUE_IS_ARRAY, $description),
            preg_match('/(.+)=\*(.+)/', $token, $matches) === 1 => new InputOption(
                $matches[1],
                $shortcut,
                InputOption::VALUE_OPTIONAL | InputOption::VALUE_IS_ARRAY,
                $description,
                preg_split('/,\s?/', $matches[2]) ?: [],
            ),
            preg_match('/(.+)=(.+)/', $token, $matches) === 1 => new InputOption($matches[1], $shortcut, InputOption::VALUE_OPTIONAL, $description, $matches[2]),
            default => new InputOption($token, $shortcut, InputOption::VALUE_NONE, $description),
        };
    }
}
