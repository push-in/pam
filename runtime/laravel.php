<?php

declare(strict_types=1);

namespace Pam\Laravel {
    use Illuminate\Contracts\Foundation\Application as ApplicationContract;
    use Illuminate\Contracts\Http\Kernel as KernelContract;
    use Illuminate\Foundation\Application as LaravelApplication;
    use Illuminate\Http\Request as LaravelRequest;
    use Illuminate\Http\UploadedFile as LaravelUploadedFile;
    use Pam\Http\Request;
    use Pam\Http\Response;
    use Pam\Http\Server;
    use Symfony\Component\HttpFoundation\BinaryFileResponse;
    use Symfony\Component\HttpFoundation\Request as SymfonyRequest;
    use Symfony\Component\HttpFoundation\Response as SymfonyResponse;
    use Symfony\Component\HttpFoundation\StreamedResponse;

    /**
     * Persistent Laravel host compiled into Pam.
     *
     * The framework is booted once. Every request receives a shallow application
     * sandbox, matching Laravel's long-running worker model without requiring a
     * server-specific Composer package.
     */
    final class Application
    {
        private const DEFAULT_MAX_RESPONSE_BYTES = 256 * 1024 * 1024;
        private const DEFAULT_MAX_RESPONSE_CHUNK_BYTES = 1024 * 1024;

        private function __construct(
            private readonly LaravelApplication $application,
        ) {
        }

        public static function boot(string $basePath): self
        {
            if (!class_exists(LaravelApplication::class)) {
                throw new \LogicException(
                    'Laravel is not installed. Run `pam composer install` in the project first.',
                );
            }

            $bootstrap = rtrim($basePath, DIRECTORY_SEPARATOR) . '/bootstrap/app.php';
            if (!is_file($bootstrap)) {
                throw new \RuntimeException("Laravel bootstrap file not found: {$bootstrap}");
            }

            $application = require $bootstrap;
            if (!$application instanceof LaravelApplication) {
                throw new \UnexpectedValueException(
                    'bootstrap/app.php must return an Illuminate\\Foundation\\Application.',
                );
            }

            $storagePath = getenv('PAM_LARAVEL_STORAGE_PATH');
            if (is_string($storagePath) && $storagePath !== '') {
                if (!is_dir($storagePath) || !is_writable($storagePath)) {
                    throw new \RuntimeException(
                        "PAM_LARAVEL_STORAGE_PATH must be a writable directory: {$storagePath}",
                    );
                }
                $application->useStoragePath($storagePath);
                self::prepareStoragePath($storagePath);
            }

            // Laravel's normal HTTP kernel binds the incoming request before it
            // boots providers. A persistent host boots once, before the first
            // network request, so packages such as Telescope still require a
            // valid bootstrap request during their service-provider lifecycle.
            $bootstrapRequest = LaravelRequest::create('http://127.0.0.1/', 'GET');
            $application->instance('request', $bootstrapRequest);
            $kernel = Sandbox::kernel($application);
            $kernel->bootstrap();
            Sandbox::warmPersistentServices($application);
            $application->forgetInstance('request');
            \Illuminate\Support\Facades\Request::clearResolvedInstance('request');
            unset($bootstrapRequest);

            return new self($application);
        }

        private static function prepareStoragePath(string $storagePath): void
        {
            foreach ([
                'app/private',
                'app/public',
                'framework/cache/data',
                'framework/sessions',
                'framework/testing',
                'framework/views',
                'logs',
            ] as $relativePath) {
                $directory = $storagePath . DIRECTORY_SEPARATOR . $relativePath;
                if (
                    !is_dir($directory)
                    && !mkdir($directory, 0775, true)
                    && !is_dir($directory)
                ) {
                    throw new \RuntimeException(
                        "Unable to create Laravel storage directory: {$directory}",
                    );
                }
            }
        }

        public static function create(string $basePath): self
        {
            return self::boot($basePath);
        }

        /** @param array<string, mixed> $options */
        public function listen(
            int $port = 3000,
            string $host = '127.0.0.1',
            array $options = [],
        ): void {
            $concurrency = $options['maxConcurrentRequests'] ?? 1;
            if ($concurrency !== 1) {
                $message = 'Laravel requires maxConcurrentRequests=1 per worker because framework managers '
                    . 'are process-global. Scale safely with `pam start --workers N`.';
                fwrite(\STDERR, "pam: {$message}\n");
                throw new \LogicException($message);
            }
            $options['maxConcurrentRequests'] = 1;
            $options['maxResponseBytes'] ??= self::DEFAULT_MAX_RESPONSE_BYTES;
            $maxResponseBytes = $options['maxResponseBytes'];
            $options['maxResponseChunkBytes'] ??= is_int($maxResponseBytes)
                ? min(self::DEFAULT_MAX_RESPONSE_CHUNK_BYTES, $maxResponseBytes)
                : self::DEFAULT_MAX_RESPONSE_CHUNK_BYTES;
            Server::create($this->handle(...))->listen($port, $host, $options);
        }

        public function handle(Request $request, Response $response): Response
        {
            $sandbox = clone $this->application;
            $laravelRequest = RequestFactory::fromPam($request);
            $laravelResponse = null;
            $outputLevel = ob_get_level();
            ob_start();

            try {
                Sandbox::activate($this->application, $sandbox, $laravelRequest);
                $kernel = Sandbox::kernel($sandbox);

                $laravelResponse = $kernel->handle($laravelRequest);
                $capturedOutput = (string) ob_get_clean();
                try {
                    return ResponseFactory::toPam(
                        $laravelRequest,
                        $laravelResponse,
                        $response,
                        $capturedOutput,
                    );
                } finally {
                    // Stream callbacks run while toPam() is suspended on native
                    // backpressure. Termination must happen after the stream has
                    // completed (or been cancelled), just like Laravel's normal
                    // HTTP kernel lifecycle.
                    $kernel->terminate($laravelRequest, $laravelResponse);
                }
            } finally {
                while (ob_get_level() > $outputLevel) {
                    ob_end_clean();
                }
                Sandbox::release(
                    $this->application,
                    $sandbox,
                    $laravelRequest,
                );
                unset($laravelRequest, $laravelResponse, $sandbox);
            }
        }

        public function laravel(): ApplicationContract
        {
            return $this->application;
        }
    }

    final class RequestFactory
    {
        public static function fromPam(Request $request): LaravelRequest
        {
            $base = new SymfonyRequest(
                query: $request->query(),
                request: $_POST,
                attributes: [],
                cookies: $_COOKIE,
                files: self::normalizeUploadedFiles($_FILES),
                server: $_SERVER,
                content: $request->body(),
            );

            return LaravelRequest::createFromBase($base);
        }

        /**
         * Pam parses multipart bodies itself and owns the temporary files. Mark
         * those files as trusted test-mode uploads so Symfony does not call
         * is_uploaded_file(), which only recognizes uploads created by a web
         * SAPI. The same conversion preserves PHP's nested multi-file shape.
         *
         * @param array<string|int, mixed> $files
         * @return array<string|int, mixed>
         */
        private static function normalizeUploadedFiles(array $files): array
        {
            $normalized = [];
            foreach ($files as $name => $file) {
                if (
                    is_array($file)
                    && array_key_exists('tmp_name', $file)
                    && array_key_exists('error', $file)
                ) {
                    $normalized[$name] = self::normalizePhpUpload($file);
                } elseif (is_array($file)) {
                    $normalized[$name] = self::normalizeUploadedFiles($file);
                } else {
                    $normalized[$name] = $file;
                }
            }

            return $normalized;
        }

        /**
         * @param array<string, mixed> $file
         * @return LaravelUploadedFile|array<array-key, mixed>|null
         */
        private static function normalizePhpUpload(array $file): LaravelUploadedFile|array|null
        {
            $temporary = $file['tmp_name'] ?? '';
            if (is_array($temporary)) {
                $uploads = [];
                foreach (array_keys($temporary) as $key) {
                    $child = [];
                    foreach (['name', 'full_path', 'type', 'tmp_name', 'error', 'size'] as $field) {
                        $values = $file[$field] ?? null;
                        $child[$field] = is_array($values) ? ($values[$key] ?? null) : $values;
                    }
                    $uploads[$key] = self::normalizePhpUpload($child);
                }

                return $uploads;
            }

            $error = $file['error'] ?? UPLOAD_ERR_NO_FILE;
            if (!is_int($error)) {
                throw new \UnexpectedValueException('Upload error must be an integer.');
            }
            if ($error === UPLOAD_ERR_NO_FILE) {
                return null;
            }

            $original = $file['full_path'] ?? $file['name'] ?? '';
            if (!is_string($temporary) || !is_string($original)) {
                throw new \UnexpectedValueException('Upload paths must be strings.');
            }
            return new LaravelUploadedFile(
                path: $temporary,
                originalName: $original,
                mimeType: is_string($file['type'] ?? null) ? $file['type'] : null,
                error: $error,
                test: true,
            );
        }
    }

    final class ResponseFactory
    {
        private const OUTPUT_CHUNK_BYTES = 64 * 1024;
        private static int $activeStreams = 0;

        public static function isStreaming(): bool
        {
            return self::$activeStreams > 0;
        }

        public static function toPam(
            LaravelRequest $request,
            SymfonyResponse $source,
            Response $target,
            string $capturedOutput = '',
        ): Response {
            $source->prepare($request);
            $target->status($source->getStatusCode());

            foreach ($source->headers->allPreserveCaseWithoutCookies() as $name => $values) {
                $normalized = strtolower($name);
                if (in_array($normalized, ['connection', 'transfer-encoding'], true)) {
                    continue;
                }
                if ($normalized === 'content-length' && $capturedOutput !== '') {
                    continue;
                }
                foreach ($values as $value) {
                    $target->addHeader($name, $value);
                }
            }
            foreach ($source->headers->getCookies() as $cookie) {
                $target->addHeader('set-cookie', (string) $cookie);
            }

            if ($request->isMethod('HEAD') || in_array($source->getStatusCode(), [204, 304], true)) {
                return $target->send('');
            }

            if ($source instanceof StreamedResponse || $source instanceof BinaryFileResponse) {
                $contentType = $source->headers->get('content-type');
                if (!is_string($contentType) || $contentType === '') {
                    $contentType = 'application/octet-stream';
                }
                $target->header('content-type', $contentType);
                self::streamContent($source, $target, $capturedOutput);

                return $target;
            }

            $content = $source->getContent();
            return $target->send($capturedOutput . ($content === false ? '' : $content));
        }

        private static function streamContent(
            StreamedResponse|BinaryFileResponse $response,
            Response $target,
            string $capturedOutput,
        ): void {
            if ($capturedOutput !== '') {
                $target->writeChunk($capturedOutput);
            }

            $outputLevel = ob_get_level();
            ++self::$activeStreams;

            try {
                ob_start(
                    static function (string $buffer) use ($target): string {
                        $target->writeChunk($buffer);
                        return '';
                    },
                    self::OUTPUT_CHUNK_BYTES,
                );
                $response->sendContent();
                if (ob_get_level() > $outputLevel) {
                    ob_end_flush();
                }
            } finally {
                while (ob_get_level() > $outputLevel) {
                    ob_end_clean();
                }
                --self::$activeStreams;
            }
        }

    }

    final class Sandbox
    {
        /**
         * Resolve Laravel's stateful managers in the root container before the
         * request sandbox is cloned. Lazy singletons first resolved inside a
         * disposable sandbox would otherwise lose database connections, array
         * cache entries, filesystem adapters, queues, and session handlers at
         * the end of that request.
         */
        public static function warmPersistentServices(LaravelApplication $application): void
        {
            foreach ([
                'auth',
                'cache',
                'db',
                'filesystem',
                'mail.manager',
                'queue',
                'session',
            ] as $binding) {
                if ($application->bound($binding) && !$application->resolved($binding)) {
                    $application->make($binding);
                }
            }
        }

        public static function kernel(LaravelApplication $application): KernelContract
        {
            $kernel = self::resolvedObject($application, KernelContract::class);
            if (!$kernel instanceof KernelContract) {
                throw new \UnexpectedValueException('Laravel HTTP kernel could not be resolved.');
            }

            return $kernel;
        }

        public static function activate(
            LaravelApplication $root,
            LaravelApplication $sandbox,
            LaravelRequest $request,
        ): void {
            \Illuminate\Container\Container::setInstance($sandbox);
            \Illuminate\Support\Facades\Facade::setFacadeApplication($sandbox);
            \Illuminate\Support\Facades\Facade::clearResolvedInstances();

            if ($sandbox->resolved('config')) {
                $sandbox->instance('config', clone self::resolvedObject($sandbox, 'config'));
            }
            if ($sandbox->resolved('url')) {
                $sandbox->instance('url', clone self::resolvedObject($sandbox, 'url'));
            }
            if ($sandbox->resolved('events')) {
                $events = self::resolvedObject($sandbox, 'events');
                if ($events instanceof \Illuminate\Events\Dispatcher) {
                    $events = clone $events;
                    self::replaceProperty($events, 'container', $sandbox);
                    $sandbox->instance('events', $events);
                }
            }
            if ($sandbox->resolved('translator')) {
                $translator = self::resolvedObject($sandbox, 'translator');
                if ($translator instanceof \Illuminate\Translation\Translator) {
                    $sandbox->instance('translator', clone $translator);
                }
            }
            if ($sandbox->resolved('validator')) {
                $validator = self::resolvedObject($sandbox, 'validator');
                if ($validator instanceof \Illuminate\Validation\Factory) {
                    $validator = clone $validator;
                    if ($sandbox->resolved('translator')) {
                        self::replaceProperty(
                            $validator,
                            'translator',
                            self::resolvedObject($sandbox, 'translator'),
                        );
                    }
                    $sandbox->instance('validator', $validator);
                }
            }
            if ($sandbox->resolved(\Illuminate\Bus\Dispatcher::class)) {
                $bus = clone self::resolvedObject(
                    $sandbox,
                    \Illuminate\Bus\Dispatcher::class,
                );
                self::replaceProperty($bus, 'container', $sandbox);
                self::replaceProperty(
                    $bus,
                    'pipeline',
                    new \Illuminate\Pipeline\Pipeline($sandbox),
                );
                $sandbox->instance(\Illuminate\Bus\Dispatcher::class, $bus);
                $sandbox->instance(\Illuminate\Contracts\Bus\Dispatcher::class, $bus);
                $sandbox->instance(\Illuminate\Contracts\Bus\QueueingDispatcher::class, $bus);
            }

            $root->instance('request', $request);
            $sandbox->instance('request', $request);
            self::pointServicesAt($sandbox);

            if (class_exists(\Illuminate\Pagination\PaginationState::class)) {
                \Illuminate\Pagination\PaginationState::resolveUsing($sandbox);
            }
        }

        public static function release(
            LaravelApplication $root,
            LaravelApplication $sandbox,
            LaravelRequest $request,
        ): void {
            try {
                $route = self::requestRoute($request);
                if ($route instanceof \Illuminate\Routing\Route) {
                    $route->flushController();
                }

                self::clearRequestState($sandbox);
            } finally {
                self::pointServicesAt($root);
                $root->forgetInstance('request');
                $sandbox->flush();
                \Illuminate\Container\Container::setInstance($root);
                \Illuminate\Support\Facades\Facade::setFacadeApplication($root);
                \Illuminate\Support\Facades\Facade::clearResolvedInstances();
            }
        }

        private static function pointServicesAt(LaravelApplication $application): void
        {
            $kernel = self::kernel($application);
            if (method_exists($kernel, 'setApplication')) {
                $kernel->setApplication($application);
            }

            self::callIfResolved($application, 'router', 'setContainer', $application);
            self::callIfResolved($application, 'view', 'setContainer', $application);
            self::callIfResolved($application, 'validator', 'setContainer', $application);
            self::callIfResolved($application, 'filesystem', 'setApplication', $application);
            self::callIfResolved($application, 'cache', 'setApplication', $application);
            self::callIfResolved($application, 'db', 'setApplication', $application);
            self::callIfResolved($application, 'log', 'setApplication', $application);
            self::callIfResolved($application, 'mail.manager', 'setApplication', $application);
            self::callIfResolved($application, 'queue', 'setApplication', $application);
            self::callIfResolved($application, 'auth', 'setApplication', $application);
            self::callIfResolved($application, 'session', 'setContainer', $application);
            self::callIfResolved($application, 'broadcast', 'setApplication', $application);
            self::callIfResolved($application, 'hash', 'setContainer', $application);
            self::callIfResolved($application, 'notifications', 'setContainer', $application);

            $events = $application->resolved('events')
                ? self::resolvedObject($application, 'events')
                : null;
            if ($events instanceof \Illuminate\Contracts\Events\Dispatcher) {
                if ($events instanceof \Illuminate\Events\Dispatcher) {
                    self::replaceProperty($events, 'container', $application);
                } elseif (method_exists($events, 'setContainer')) {
                    $events->setContainer($application);
                }
                \Illuminate\Database\Eloquent\Model::setEventDispatcher($events);
            }

            if ($application->resolved('validator')) {
                $validator = self::resolvedObject($application, 'validator');
                if (
                    $validator instanceof \Illuminate\Validation\Factory
                    && $application->resolved('translator')
                ) {
                    self::replaceProperty(
                        $validator,
                        'translator',
                        self::resolvedObject($application, 'translator'),
                    );
                }
            }

            if ($application->resolved(\Illuminate\Bus\Dispatcher::class)) {
                $bus = self::resolvedObject($application, \Illuminate\Bus\Dispatcher::class);
                self::replaceProperty($bus, 'container', $application);
                self::replaceProperty(
                    $bus,
                    'pipeline',
                    new \Illuminate\Pipeline\Pipeline($application),
                );
            }

            if ($application->resolved('view')) {
                $view = self::resolvedObject($application, 'view');
                if (method_exists($view, 'share')) {
                    $view->share('app', $application);
                }
                if ($events instanceof \Illuminate\Contracts\Events\Dispatcher) {
                    self::replaceProperty($view, 'events', $events);
                }
            }

            if (
                $application->resolved('router')
                && $events instanceof \Illuminate\Contracts\Events\Dispatcher
            ) {
                self::replaceProperty(
                    self::resolvedObject($application, 'router'),
                    'events',
                    $events,
                );
            }

            if ($application->resolved('routes')) {
                $routes = self::resolvedObject($application, 'routes');
                if (method_exists($routes, 'setContainer')) {
                    $routes->setContainer($application);
                }
                if ($routes instanceof \Traversable) {
                    foreach ($routes as $route) {
                        if (is_object($route) && method_exists($route, 'setContainer')) {
                            $route->setContainer($application);
                        }
                    }
                }
            }
        }

        private static function clearRequestState(LaravelApplication $application): void
        {
            if ($application->resolved('view')) {
                $view = self::resolvedObject($application, 'view');
                if (method_exists($view, 'flushState')) {
                    $view->flushState();
                }
            }

            if ($application->resolved('cookie')) {
                $cookies = self::resolvedObject($application, 'cookie');
                if (method_exists($cookies, 'flushQueuedCookies')) {
                    $cookies->flushQueuedCookies();
                }
            }

            if ($application->resolved('auth')) {
                $auth = self::resolvedObject($application, 'auth');
                if (method_exists($auth, 'forgetGuards')) {
                    $auth->forgetGuards();
                }
            }

            if ($application->resolved('session')) {
                $sessions = self::resolvedObject($application, 'session');
                if (method_exists($sessions, 'getDrivers')) {
                    foreach ($sessions->getDrivers() as $session) {
                        if (is_object($session) && method_exists($session, 'flush')) {
                            // StartSession has already persisted the attributes.
                            // Clear only the reusable Store object so a request
                            // without a cookie cannot inherit the prior session.
                            $session->flush();
                        }
                    }
                }
            }

            if ($application->resolved('db')) {
                $database = self::resolvedObject($application, 'db');
                if (method_exists($database, 'getConnections')) {
                    foreach ($database->getConnections() as $connection) {
                        if (!is_object($connection)) {
                            continue;
                        }
                        if (method_exists($connection, 'setEventDispatcher')) {
                            $connection->setEventDispatcher(
                                self::resolvedObject($application, 'events'),
                            );
                        }
                        foreach ([
                            'flushQueryLog',
                            'forgetRecordModificationState',
                            'resetTotalQueryDuration',
                            'allowQueryDurationHandlersToRunAgain',
                        ] as $method) {
                            if (method_exists($connection, $method)) {
                                $connection->{$method}();
                            }
                        }
                    }
                }
            }

            if ($application->resolved('log')) {
                $log = self::resolvedObject($application, 'log');
                foreach (['flushSharedContext', 'withoutContext'] as $method) {
                    if (method_exists($log, $method)) {
                        $log->{$method}();
                    }
                }
            }

            foreach ([
                \Illuminate\Support\Once::class => 'flush',
                \Illuminate\Support\Str::class => 'flushCache',
                \Illuminate\Validation\Validator::class => 'flushState',
                \Illuminate\Foundation\Http\FormRequest::class => 'flushState',
                \Illuminate\Http\Client\Response::class => 'flushState',
                \Illuminate\Http\Resources\Json\JsonResource::class => 'flushState',
            ] as $class => $method) {
                if (class_exists($class) && method_exists($class, $method)) {
                    $class::{$method}();
                }
            }

            if (method_exists($application, 'resetScope')) {
                $application->resetScope();
            }
            $application->forgetScopedInstances();
            $application->forgetInstance('request');
            \Illuminate\Support\Facades\Request::clearResolvedInstance('request');
        }

        private static function callIfResolved(
            LaravelApplication $application,
            string $binding,
            string $method,
            LaravelApplication $argument,
        ): void {
            if (!$application->resolved($binding)) {
                return;
            }
            $service = self::resolvedObject($application, $binding);
            if (method_exists($service, $method)) {
                $service->{$method}($argument);
            }
        }

        private static function resolvedObject(
            LaravelApplication $application,
            string $binding,
        ): object {
            $service = $application->make($binding);
            if (!is_object($service)) {
                throw new \UnexpectedValueException("Laravel binding {$binding} must resolve to an object.");
            }

            return $service;
        }

        private static function replaceProperty(
            object $service,
            string $property,
            mixed $value,
        ): void {
            static $properties = [];

            $key = $service::class . "\0" . $property;
            $reflection = $properties[$key] ??= new \ReflectionProperty($service, $property);
            $reflection->setValue($service, $value);
        }

        private static function requestRoute(LaravelRequest $request): mixed
        {
            return $request->route();
        }
    }
}
