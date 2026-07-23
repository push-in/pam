# pushinbr/pam-api

The optional Express-like HTTP layer for Pam: route parameters, 404/405 handling,
a precompiled middleware pipeline, error boundaries and Composer provider
discovery.

```bash
pam composer require pushinbr/pam-api
```

```php
use Pam\App;

$app = new App();
$app->get('/users/{id}', static fn ($request, $response) =>
    $response->json(['id' => $request->route('id')]));
$app->listen(3000);
```
