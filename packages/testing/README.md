# pam/testing

Fast in-memory tests for applications built with `pam/api`.

```bash
composer require --dev pam/testing
```

```php
$client = new Pam\Testing\TestClient($app);
$client->get('/users/42')
    ->assertSuccessful()
    ->assertJsonPath('id', '42');
```

The client invokes the application pipeline without opening a network port. Use
Pam's Rust integration suite for transport, timeout and protocol behavior.
