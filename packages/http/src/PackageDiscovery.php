<?php

declare(strict_types=1);

namespace Pam\Api;

use Pam\Contracts\Package\ServiceProviderInterface;

final class PackageDiscovery
{
    /** @return list<class-string<ServiceProviderInterface>> */
    public static function providers(string $projectRoot): array
    {
        if (getenv('PAM_DISABLE_PACKAGE_DISCOVERY') === '1') {
            return [];
        }

        $installed = rtrim($projectRoot, DIRECTORY_SEPARATOR) . '/vendor/composer/installed.json';
        if (!is_file($installed)) {
            return [];
        }

        $cacheDirectory = rtrim($projectRoot, DIRECTORY_SEPARATOR) . '/.pam/cache';
        $cache = $cacheDirectory . '/packages.json';
        $lock = rtrim($projectRoot, DIRECTORY_SEPARATOR) . '/composer.lock';
        $sourceModified = max((int) filemtime($installed), is_file($lock) ? (int) filemtime($lock) : 0);
        if (is_file($cache) && (int) filemtime($cache) >= $sourceModified) {
            $cached = file_get_contents($cache);
            if (!is_string($cached)) {
                throw new \RuntimeException("Unable to read Pam package cache at {$cache}.");
            }
            $providers = json_decode($cached, true, 32, JSON_THROW_ON_ERROR);
            return self::validateProviders($providers);
        }

        $contents = file_get_contents($installed);
        if (!is_string($contents)) {
            throw new \RuntimeException("Unable to read Composer package metadata at {$installed}.");
        }
        $installedData = json_decode($contents, true, 512, JSON_THROW_ON_ERROR);
        $versions = is_array($installedData) && is_array($installedData['packages'] ?? null)
            ? $installedData['packages']
            : (is_array($installedData) ? $installedData : []);
        $providers = [];
        foreach ($versions as $package) {
            if (!is_array($package)) {
                continue;
            }
            $extra = is_array($package['extra'] ?? null) ? $package['extra'] : [];
            $pam = is_array($extra['pam'] ?? null) ? $extra['pam'] : [];
            $configured = is_array($pam['providers'] ?? null) ? $pam['providers'] : [];
            foreach ($configured as $provider) {
                if (is_string($provider)) {
                    $providers[] = $provider;
                }
            }
        }
        $providers = self::validateProviders($providers);

        if (!is_dir($cacheDirectory) && !mkdir($cacheDirectory, 0755, true) && !is_dir($cacheDirectory)) {
            throw new \RuntimeException("Unable to create Pam package cache at {$cacheDirectory}.");
        }
        $temporary = $cache . '.' . getmypid() . '.tmp';
        $contents = json_encode($providers, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        if (file_put_contents($temporary, $contents, LOCK_EX) === false || !rename($temporary, $cache)) {
            @unlink($temporary);
            throw new \RuntimeException("Unable to write Pam package cache at {$cache}.");
        }

        return $providers;
    }

    /**
     * @param mixed $providers
     * @return list<class-string<ServiceProviderInterface>>
     */
    private static function validateProviders(mixed $providers): array
    {
        if (!is_array($providers)) {
            throw new \RuntimeException('Pam package discovery returned an invalid provider list.');
        }
        $validated = [];
        foreach ($providers as $provider) {
            if (!is_string($provider) || !class_exists($provider)) {
                throw new \RuntimeException('A discovered Pam service provider is unavailable.');
            }
            if (!is_subclass_of($provider, ServiceProviderInterface::class)) {
                throw new \RuntimeException("Pam provider {$provider} must implement ServiceProviderInterface.");
            }
            $validated[] = $provider;
        }
        return array_values(array_unique($validated));
    }
}
