# pushinbr/pam-testing

Fast in-memory tests for applications built with `pushinbr/pam-api`.

```bash
pam composer require --dev pushinbr/pam-testing
```

```php
$client = new Pam\Testing\TestClient($app);
$client->get('/users/42')
    ->assertSuccessful()
    ->assertJsonPath('id', '42');
```

The client invokes the application pipeline without opening a network port. Use
Pam's Rust integration suite for transport, timeout and protocol behavior.

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.
