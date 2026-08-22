# pushinbr/pam-testing

Fast in-memory tests for applications built with `pushinbr/pam-http`.

## Start here

PAM Testing is a Composer package for applications running on the PAM Runtime;
it is not a standalone test runtime. Install PAM first, open your application
directory, and add the development package through PAM's Composer toolchain:

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

pam doctor
cd my-app
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
